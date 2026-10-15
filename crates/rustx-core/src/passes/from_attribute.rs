//! `#[from]` on a variant's field: how the attribute macros spell `from` variants (problem 05).
//!
//! `Io(from io::Error)` does not parse as Rust, so `#[display]` and `#[sugared]` take
//! `Io(#[from] io::Error)` instead and turn it into the marker `from_display`'s token half leaves,
//! so `from_display::lower` generates the same `From` impl. The attribute sits where `from` sits in
//! the sugar, on the field, which is also where thiserror and `derive_more` put theirs.
//!
//! An enum that derives `Error` or `From` keeps its `#[from]` attributes: they belong to that
//! derive, and a second `From` impl would conflict with the derived one.

use proc_macro2::{Ident, Span};
use syn::spanned::Spanned;
use syn::visit_mut::{self, VisitMut};
use syn::{Fields, ItemEnum, Meta, Variant};

use super::from_display::FROM_MARKER;
use crate::tokens::syntax::{combine, derived_paths, take_attr};

const ATTR: &str = "from";

const SHAPE: &str =
    "`#[from]` needs a tuple variant with exactly one field, as in `Io(#[from] io::Error)`";

pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    let mut pass = Lower { errors: None };
    pass.visit_file_mut(file);
    pass.errors.map_or(Ok(()), Err)
}

struct Lower {
    errors: Option<syn::Error>,
}

impl VisitMut for Lower {
    fn visit_item_enum_mut(&mut self, item: &mut ItemEnum) {
        let owned = derived_paths(&item.attrs)
            .filter_map(|path| path.segments.last().map(|last| last.ident.to_string()))
            .any(|name| name == "Error" || name == "From");
        if !owned {
            for variant in &mut item.variants {
                self.variant(variant);
            }
        }
        visit_mut::visit_item_enum_mut(self, item);
    }
}

impl Lower {
    fn fail(&mut self, span: Span, msg: &str) {
        combine(&mut self.errors, syn::Error::new(span, msg));
    }

    fn variant(&mut self, variant: &mut Variant) {
        while let Some(attr) = take_attr(&mut variant.attrs, ATTR) {
            let msg = "`#[from]` goes on the variant's field, as in `Io(#[from] io::Error)`";
            self.fail(attr.path().span(), msg);
        }
        let one_field =
            matches!(&variant.fields, Fields::Unnamed(fields) if fields.unnamed.len() == 1);
        let mut marks = Vec::new();
        for field in &mut variant.fields {
            while let Some(attr) = take_attr(&mut field.attrs, ATTR) {
                marks.push(attr);
            }
        }
        match &marks[..] {
            [] => {}
            [attr] if one_field => {
                let span = attr.path().span();
                if !matches!(attr.meta, Meta::Path(_)) {
                    self.fail(span, "`#[from]` takes no arguments");
                }
                let marker = Ident::new(FROM_MARKER, span);
                variant
                    .attrs
                    .push(syn::parse_quote_spanned!(span=> #[#marker]));
            }
            _ => {
                for attr in marks {
                    self.fail(attr.path().span(), SHAPE);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro2::TokenStream;
    use quote::{ToTokens, quote};

    fn lowered(source: TokenStream) -> syn::Result<String> {
        let mut file: syn::File = syn::parse2(source)?;
        lower(&mut file)?;
        Ok(file.into_token_stream().to_string())
    }

    fn printed(plain: TokenStream) -> String {
        syn::parse2::<syn::File>(plain)
            .unwrap()
            .into_token_stream()
            .to_string()
    }

    #[test]
    fn from_on_a_single_field_becomes_the_marker_at_any_depth() {
        assert_eq!(
            lowered(quote! {
                enum E { #[display("io")] Io(#[from] io::Error), Raw(u8) }
                mod m { fn f() { enum F { A(#[x] #[from] u8) } } }
            })
            .unwrap(),
            printed(quote! {
                enum E { #[display("io")] #[__rustx_from] Io(io::Error), Raw(u8) }
                mod m { fn f() { enum F { #[__rustx_from] A(#[x] u8) } } }
            })
        );
    }

    #[test]
    fn an_enum_deriving_error_or_from_keeps_its_attributes() {
        let owned = quote! {
            #[derive(Debug, thiserror::Error)]
            enum E { A(#[from] io::Error) }
            #[derive(derive_more::From)]
            enum F { B(#[from] u8) }
        };
        assert_eq!(lowered(owned.clone()).unwrap(), printed(owned));
    }

    #[test]
    fn misplaced_from_is_reported_together() {
        let errors: Vec<String> = lowered(quote! {
            enum E {
                #[from] A(u8),
                B(#[from] u8, #[from] u16),
                C { #[from] e: u8 },
                D(#[from(x)] u8),
            }
        })
        .unwrap_err()
        .into_iter()
        .map(|err| err.to_string())
        .collect();
        assert_eq!(
            errors,
            [
                "`#[from]` goes on the variant's field, as in `Io(#[from] io::Error)`",
                SHAPE,
                SHAPE,
                SHAPE,
                "`#[from]` takes no arguments",
            ]
        );
    }
}
