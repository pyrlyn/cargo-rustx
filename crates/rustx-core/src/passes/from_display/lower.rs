//! AST half: the `From`, `Display` and `Error` impls of each enum, placed right after it.
//!
//! Generated impls are real items in the same list as the enum, so the passes after this one
//! (`inline_bounds`, `#[borrows]`) see and extend them like hand-written impls.

use std::collections::HashMap;

use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, quote, quote_spanned};
use syn::ext::IdentExt;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Attribute, Fields, Generics, Item, ItemEnum, LitStr, Token};

use super::{FROM_MARKER, FROM_SHAPE, format};
use crate::passes::item_lists::{Entry, ExpandList, for_each_list, insert_after};
use crate::tokens::syntax::{combine, derived_paths, introduced_span, take_attr};

pub(in crate::passes) fn lower(file: &mut syn::File) -> syn::Result<()> {
    let mut pass = Lower { errors: None };
    for_each_list(file, &mut pass);
    pass.errors.map_or(Ok(()), Err)
}

const DISPLAY_ARGS: &str = "expected one format string, as in `#[display(\"io error: {0}\")]`";

struct Lower {
    errors: Option<syn::Error>,
}

/// The enum an impl is for: its name and generics, copied into every impl header.
struct Header<'a> {
    ident: &'a Ident,
    generics: &'a Generics,
}

impl Header<'_> {
    fn implement(&self, span: Span, tr: &TokenStream, body: &TokenStream) -> TokenStream {
        let ident = self.ident;
        let (impl_generics, type_generics, where_clause) = self.generics.split_for_impl();
        quote_spanned! {span=>
            impl #impl_generics #tr for #ident #type_generics #where_clause { #body }
        }
    }
}

impl Lower {
    fn generate(&mut self, item: &mut Item) -> Vec<Item> {
        let Item::Enum(item) = item else {
            return Vec::new();
        };
        let derived = derived_names(&item.attrs);
        let ItemEnum {
            ident,
            generics,
            variants,
            ..
        } = item;
        let header = Header { ident, generics };
        let mut impls = self.conversions(&header, variants);
        // A derived `Display` owns the `#[display]` attributes; leave them to it.
        if !derived.iter().any(|name| name == "Display") {
            match display_impl(&header, variants) {
                Ok(None) => {}
                Ok(Some((span, display))) => {
                    impls.push(display);
                    if !derived.iter().any(|name| name == "Error") {
                        let error = quote_spanned!(span=> ::std::error::Error);
                        impls.push(header.implement(span, &error, &TokenStream::new()));
                    }
                }
                Err(err) => combine(&mut self.errors, err),
            }
        }
        impls
            .into_iter()
            .filter_map(|tokens| {
                syn::parse2(tokens)
                    .map_err(|err| combine(&mut self.errors, err))
                    .ok()
            })
            .collect()
    }

    fn conversions(
        &mut self,
        header: &Header,
        variants: &mut Punctuated<syn::Variant, Token![,]>,
    ) -> Vec<TokenStream> {
        let mut impls = Vec::new();
        let mut sources: HashMap<String, Ident> = HashMap::new();
        for variant in variants.iter_mut() {
            let Some(marker) = take_attr(&mut variant.attrs, FROM_MARKER) else {
                continue;
            };
            let span = marker.path().span();
            // The token half only marks one-field tuple variants; a hand-written marker may not.
            let ty = match &variant.fields {
                Fields::Unnamed(fields) if fields.unnamed.len() == 1 => &fields.unnamed[0].ty,
                _ => {
                    combine(&mut self.errors, syn::Error::new(span, FROM_SHAPE));
                    continue;
                }
            };
            let key = ty.to_token_stream().to_string();
            if let Some(first) = sources.get(&key) {
                let msg = format!(
                    "`{first}` already converts from this type; `from` takes each type once"
                );
                combine(&mut self.errors, syn::Error::new_spanned(ty, msg));
                continue;
            }
            sources.insert(key, variant.ident.clone());
            let value = Ident::new("__rustx_value", introduced_span(span));
            let name = &variant.ident;
            impls.push(header.implement(
                span,
                &quote_spanned!(span=> ::core::convert::From<#ty>),
                &quote_spanned! {span=>
                    fn from(#value: #ty) -> Self { Self::#name(#value) }
                },
            ));
        }
        impls
    }
}

/// `impl Display` from the variants' `#[display]` attributes, which it removes, and the span of
/// the first one; `None` when no variant has one.
fn display_impl(
    header: &Header,
    variants: &mut Punctuated<syn::Variant, Token![,]>,
) -> syn::Result<Option<(Span, TokenStream)>> {
    let attrs: Vec<Option<Attribute>> = variants
        .iter_mut()
        .map(|variant| take_attr(&mut variant.attrs, "display"))
        .collect();
    let Some(span) = attrs.iter().flatten().next().map(|attr| attr.path().span()) else {
        return Ok(None);
    };
    let f = Ident::new("__rustx_f", introduced_span(span));
    let mut arms = Vec::new();
    let mut errors = None;
    for (variant, attr) in variants.iter().zip(&attrs) {
        let Some(attr) = attr else {
            let msg = format!(
                "`{}` has no `#[display(..)]`, but other variants do; every variant needs one",
                variant.ident
            );
            combine(&mut errors, syn::Error::new_spanned(&variant.ident, msg));
            continue;
        };
        match display_arm(variant, attr, &f) {
            Ok(arm) => arms.push(arm),
            Err(err) => combine(&mut errors, err),
        }
    }
    if let Some(errors) = errors {
        return Err(errors);
    }
    let body = quote_spanned! {span=>
        fn fmt(&self, #f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self { #(#arms)* }
        }
    };
    let display = quote_spanned!(span=> ::core::fmt::Display);
    Ok(Some((span, header.implement(span, &display, &body))))
}

/// `Self::V(a, _) => ::core::write!(f, "..", a),` for one variant.
fn display_arm(variant: &syn::Variant, attr: &Attribute, f: &Ident) -> syn::Result<TokenStream> {
    let lit: LitStr = attr
        .parse_args()
        .map_err(|_| syn::Error::new_spanned(attr, DISPLAY_ARGS))?;
    let name = &variant.ident;
    let field = |arg: &str| match &variant.fields {
        Fields::Unnamed(fields) => arg.parse().ok().filter(|&i| i < fields.unnamed.len()),
        Fields::Named(fields) => fields
            .named
            .iter()
            .position(|field| field.ident.as_ref().is_some_and(|id| id.unraw() == arg)),
        Fields::Unit => None,
    };
    let (text, used) = format::rewrite(&lit.value(), &name.to_string(), field)
        .map_err(|msg| syn::Error::new(lit.span(), msg))?;
    let format = LitStr::new(&text, lit.span());
    let args: Vec<Ident> = (0..used.len())
        .map(|k| Ident::new(&format!("__rustx_arg{k}"), introduced_span(lit.span())))
        .collect();
    let binding = |index: usize| used.iter().position(|&u| u == index).map(|k| &args[k]);
    let pattern = match &variant.fields {
        Fields::Unit => quote!(Self::#name),
        Fields::Unnamed(fields) => {
            let elems = (0..fields.unnamed.len())
                .map(|index| binding(index).map_or_else(|| quote!(_), ToTokens::to_token_stream));
            quote!(Self::#name(#(#elems),*))
        }
        Fields::Named(fields) => {
            let elems = used.iter().zip(&args).map(|(&index, arg)| {
                let ident = &fields.named[index].ident;
                quote!(#ident: #arg)
            });
            quote!(Self::#name { #(#elems,)* .. })
        }
    };
    Ok(quote_spanned! {lit.span()=>
        #pattern => ::core::write!(#f, #format #(, #args)*),
    })
}

/// The last path segment of every trait in the enum's `#[derive(..)]` lists.
fn derived_names(attrs: &[Attribute]) -> Vec<String> {
    derived_paths(attrs)
        .filter_map(|path| Some(path.segments.last()?.ident.to_string()))
        .collect()
}

impl ExpandList for Lower {
    fn expand_list<T: Entry>(&mut self, list: &mut Vec<T>) {
        insert_after(list, |item| self.generate(item));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn errors(source: TokenStream) -> Vec<String> {
        lowered(source)
            .unwrap_err()
            .into_iter()
            .map(|err| err.to_string())
            .collect()
    }

    #[test]
    fn from_variants_get_from_impls_with_the_enum_generics() {
        assert_eq!(
            lowered(quote! {
                enum E<T: Copy, const N: usize = 1> where T: Default {
                    #[__rustx_from] A(io::Error),
                    #[__rustx_from] B([T; N]),
                    C(u8),
                }
            })
            .unwrap(),
            printed(quote! {
                enum E<T: Copy, const N: usize = 1> where T: Default { A(io::Error), B([T; N]), C(u8), }
                impl<T: Copy, const N: usize> ::core::convert::From<io::Error> for E<T, N>
                where T: Default {
                    fn from(__rustx_value: io::Error) -> Self { Self::A(__rustx_value) }
                }
                impl<T: Copy, const N: usize> ::core::convert::From<[T; N]> for E<T, N>
                where T: Default {
                    fn from(__rustx_value: [T; N]) -> Self { Self::B(__rustx_value) }
                }
            })
        );
    }

    #[test]
    fn display_writes_only_the_named_fields_and_adds_error() {
        assert_eq!(
            lowered(quote! {
                mod m {
                    #[derive(Debug)]
                    enum E {
                        #[display("io: {0}")] Io(io::Error),
                        #[display("{b} of {a:?}")] Pair { a: u8, r#b: u16, c: u32 },
                        #[display("last {1}")] Two(u8, u16),
                        #[display("none")] None,
                    }
                }
            })
            .unwrap(),
            printed(quote! {
                mod m {
                    #[derive(Debug)]
                    enum E { Io(io::Error), Pair { a: u8, r#b: u16, c: u32 }, Two(u8, u16), None, }
                    impl ::core::fmt::Display for E {
                        fn fmt(&self, __rustx_f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                            match self {
                                Self::Io(__rustx_arg0) => ::core::write!(__rustx_f, "io: {0}", __rustx_arg0),
                                Self::Pair { r#b: __rustx_arg0, a: __rustx_arg1, .. } =>
                                    ::core::write!(__rustx_f, "{0} of {1:?}", __rustx_arg0, __rustx_arg1),
                                Self::Two(_, __rustx_arg0) => ::core::write!(__rustx_f, "last {0}", __rustx_arg0),
                                Self::None => ::core::write!(__rustx_f, "none"),
                            }
                        }
                    }
                    impl ::std::error::Error for E {}
                }
            })
        );
    }

    #[test]
    fn enums_in_function_bodies_get_impls_next_to_them() {
        let out = lowered(quote! {
            fn f() { enum E { #[__rustx_from] A(u8) } let x = 1; }
        })
        .unwrap();
        let impl_at = out.find("impl").unwrap();
        assert!(impl_at < out.find("let x").unwrap(), "{out}");
    }

    #[test]
    fn derived_display_keeps_its_attributes_and_derived_error_gets_no_impl() {
        let derived = quote! {
            #[derive(Debug, derive_more::Display)]
            enum E { #[display("a")] A }
        };
        assert_eq!(lowered(derived.clone()).unwrap(), printed(derived));
        let out = lowered(quote! {
            #[derive(Debug, thiserror::Error)]
            enum E { #[display("a")] A }
        })
        .unwrap();
        assert!(
            out.contains("Display") && !out.contains(":: std :: error"),
            "{out}"
        );
    }

    #[test]
    fn valid_rust_is_unchanged() {
        let plain = quote! {
            #[derive(Debug)]
            enum E { A(u8), #[other] B { x: u8 } }
            mod m { fn f() { enum F { C } } }
        };
        assert_eq!(lowered(plain.clone()).unwrap(), printed(plain));
    }

    #[test]
    fn misuses_are_reported_together() {
        assert_eq!(
            errors(quote! {
                enum E {
                    #[__rustx_from] #[display("a {0}")] A(io::Error),
                    #[__rustx_from] #[display("b {x}")] B(io::Error),
                    C,
                    #[display(1)] D,
                }
            }),
            [
                "`A` already converts from this type; `from` takes each type once",
                "`{x}` names no field of `B`",
                "`C` has no `#[display(..)]`, but other variants do; every variant needs one",
                DISPLAY_ARGS,
            ]
        );
    }
}
