//! `derive` presets: `derive Id = Debug, Clone;` expands inside every `#[derive(..)]` (problem 18).
//!
//! Two halves, because the declaration is not Rust but expanding it needs the parsed items:
//!
//! 1. Token pass ([`marker`]): `derive Name = A, B, path::C;` at item position is removed and
//!    recorded as an inner attribute `#![__rustx_derive_preset(Name = A, B, path::C)]` at the
//!    start of the input.
//! 2. AST pass ([`lower`]): takes the markers off, resolves presets that use other presets, and
//!    rewrites the `#[derive(..)]` of every struct, enum and union.
//!
//! ```text
//! derive Id = Debug, Clone, Copy;                       #[derive(Debug, Clone, Copy)]
//! #[derive(Id, Clone, serde::Serialize)] struct A;  →   struct A;  // with `serde::Serialize` added
//! ```
//!
//! # Why a file-level attribute
//!
//! A marker left at the declaration's position would have to be an item, a statement or an impl
//! item depending on where the declaration stood, and each of those parses differently. An inner
//! attribute at the start of the file parses the same for every position, and presets are one
//! flat namespace for the whole input anyway: declaring one in a nested module makes it usable
//! everywhere in the input, and a name declared twice is an error.
//!
//! # Rules
//!
//! - **Expansion.** In a `#[derive(..)]` list, a path that is exactly a preset name (one segment,
//!   no leading `::`) becomes the preset's list. A list that uses a preset is then deduplicated
//!   keeping the first occurrence; lists without a preset are left exactly as written.
//! - **Nested presets.** A preset may use presets declared anywhere in the input; a cycle is an
//!   error.
//! - **Local only.** Presets exist inside one `sugar!` input, so a visibility is an error instead
//!   of being silently ignored. A preset takes no attributes other than docs, which are dropped.
//! - **Shadowing.** A preset named like a std derive (`Debug`, `Clone`, ..) is an error: the
//!   same `#[derive(Debug)]` would mean different things in different inputs.
//! - **`cfg_attr`.** `#[cfg_attr(cond, derive(Id))]` is expanded the same way, one level deep.
//!
//! # Limitations
//!
//! - Duplicates are found by the written path, so `Debug` and `std::fmt::Debug` are both kept
//!   and rustc reports the conflicting impls.
//! - Several `#[derive]` attributes on one item are expanded independently, so a derive named in
//!   two of them is not deduplicated across the attributes.
//! - A preset named like a derive macro imported into the input (`use serde::Serialize;`)
//!   shadows it for the whole input; only the std names are rejected.
//! - Derives written inside macro bodies and nested `cfg_attr`s are not expanded.

mod lower;
mod marker;

use proc_macro2::TokenStream;
use syn::parse::{Parse, ParseStream};
use syn::{Ident, Path, Token};

/// Name of the inner attribute that carries a preset from the token half to the AST half.
const MARKER: &str = "__rustx_derive_preset";

/// A parsed `Name = A, B, path::C`, without the keyword and the `;`.
struct Preset {
    name: Ident,
    derives: Vec<Path>,
}

impl Parse for Preset {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        let Ok(eq) = input.parse::<Token![=]>() else {
            return Err(input
                .error("expected `=` after the preset name, as in `derive Id = Debug, Clone;`"));
        };
        let mut derives = Vec::new();
        // Stops at the first token that cannot start a path instead of requiring end of input,
        // so the caller decides what may follow the list (`;` in source, nothing in a marker).
        while input.peek(Ident) || input.peek(Token![::]) {
            derives.push(Path::parse_mod_style(input)?);
            if input.parse::<Token![,]>().is_err() {
                break;
            }
        }
        if derives.is_empty() {
            return Err(syn::Error::new(
                eq.span,
                "expected at least one derive after `=`, as in `derive Id = Debug, Clone;`",
            ));
        }
        Ok(Self { name, derives })
    }
}

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    marker::rewrite(tokens)
}

pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    lower::lower(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::{ToTokens, quote};

    fn desugar(sugar: TokenStream) -> syn::Result<String> {
        let mut file: syn::File = syn::parse2(rewrite(sugar)?)?;
        lower(&mut file)?;
        Ok(file.into_token_stream().to_string())
    }

    fn reprint(plain: TokenStream) -> String {
        syn::parse2::<syn::File>(plain)
            .unwrap()
            .into_token_stream()
            .to_string()
    }

    fn errors(sugar: TokenStream) -> Vec<String> {
        desugar(sugar)
            .err()
            .into_iter()
            .flatten()
            .map(|err| err.to_string())
            .collect()
    }

    fn assert_expands(sugar: TokenStream, plain: TokenStream) {
        assert_eq!(desugar(sugar).unwrap(), reprint(plain));
    }

    #[test]
    fn a_preset_expands_in_structs_enums_and_unions() {
        assert_expands(
            quote! {
                derive Id = Debug, Clone, Copy;
                #[derive(Id)] struct A(u8);
                #[derive(Id)] enum B { X }
                #[derive(Id)] union C { a: u8 }
            },
            quote! {
                #[derive(Debug, Clone, Copy)] struct A(u8);
                #[derive(Debug, Clone, Copy)] enum B { X }
                #[derive(Debug, Clone, Copy)] union C { a: u8 }
            },
        );
    }

    #[test]
    fn presets_mix_with_ordinary_derives_and_are_deduplicated_in_order() {
        assert_expands(
            quote! {
                derive Id = Debug, Clone, Copy;
                #[derive(Clone, Id, serde::Serialize, Debug)] struct A;
            },
            quote! { #[derive(Clone, Debug, Copy, serde::Serialize)] struct A; },
        );
    }

    #[test]
    fn a_preset_may_use_presets_declared_later_or_in_modules() {
        assert_expands(
            quote! {
                derive Value = Id, Hash;
                mod m { derive Id = Debug, Eq; }
                #[derive(Value)] struct A;
            },
            quote! { mod m {} #[derive(Debug, Eq, Hash)] struct A; },
        );
    }

    #[test]
    fn presets_work_in_nested_modules_and_function_bodies() {
        assert_expands(
            quote! {
                derive Id = Debug;
                mod m { #[derive(Id)] pub struct A; }
                fn f() { #[derive(Id)] struct B; }
            },
            quote! {
                mod m { #[derive(Debug)] pub struct A; }
                fn f() { #[derive(Debug)] struct B; }
            },
        );
    }

    #[test]
    fn a_trailing_comma_and_leading_colons_are_accepted() {
        assert_expands(
            quote! { derive Id = Debug, ::core::clone::Clone,; #[derive(Id)] struct A; },
            quote! { #[derive(Debug, ::core::clone::Clone)] struct A; },
        );
    }

    #[test]
    fn cfg_attr_derives_are_expanded() {
        assert_expands(
            quote! {
                derive Id = Debug, Clone;
                #[cfg_attr(feature = "x", derive(Id, Debug), allow(dead_code))] struct A;
                #[cfg_attr(unix, derive(Copy))] struct B;
            },
            quote! {
                #[cfg_attr(feature = "x", derive(Debug, Clone), allow(dead_code))] struct A;
                #[cfg_attr(unix, derive(Copy))] struct B;
            },
        );
    }

    #[test]
    fn lists_without_a_preset_are_left_alone() {
        assert_expands(
            quote! {
                derive Id = Debug;
                #[derive(Clone, Clone)] struct A;
                #[derive(Idx)] struct B;
                #[derive(a::Id)] struct C;
                #[derive(::Id)] struct D;
            },
            quote! {
                #[derive(Clone, Clone)] struct A;
                #[derive(Idx)] struct B;
                #[derive(a::Id)] struct C;
                #[derive(::Id)] struct D;
            },
        );
    }

    #[test]
    fn input_without_presets_is_untouched() {
        let plain = quote! { #[derive(Debug, Debug)] struct A; fn derive(x: u8) { derive(x) } };
        assert_expands(plain.clone(), plain);
    }

    #[test]
    fn derive_as_an_identifier_passes_through() {
        let plain = quote! {
            fn derive(derive: u8) -> u8 { derive }
            fn g(s: S) {
                let derive = 1;
                derive = 2;
                derive(1);
                derive as u8;
                s.derive;
                derive!(x);
                match derive { derive => derive }
            }
            #[derive(Debug)] struct S { derive: u8 }
        };
        assert_expands(plain.clone(), plain);
    }

    #[test]
    fn docs_on_a_preset_are_dropped() {
        assert_expands(
            quote! { /// An id.
            derive Id = Debug; #[derive(Id)] struct A; },
            quote! { #[derive(Debug)] struct A; },
        );
    }

    #[test]
    fn syntax_errors_are_reported_on_the_user_tokens() {
        let cases = [
            (
                quote! { derive Id Debug; },
                "expected `=` after the preset name",
            ),
            (quote! { derive Id; }, "expected `=` after the preset name"),
            (quote! { derive Id = ; }, "expected at least one derive"),
            (
                quote! { derive Id = Debug Clone; },
                "expected `;` after the derive list",
            ),
            (
                quote! { derive Id = Debug, Clone },
                "expected `;` after the derive list",
            ),
            (
                quote! { derive Id = Debug, Clone struct S; },
                "expected `;` after the derive list",
            ),
        ];
        for (input, message) in cases {
            let errs = errors(input.clone());
            assert!(
                errs.len() == 1 && errs[0].starts_with(message),
                "{input}: {errs:?}"
            );
        }
    }

    #[test]
    fn std_names_visibility_and_attributes_are_rejected() {
        let errs = errors(quote! { derive Debug = Clone; });
        assert!(
            errs[0].contains("shadows the std derive `Debug`"),
            "{errs:?}"
        );
        let errs = errors(quote! { pub derive Id = Debug; });
        assert!(errs[0].contains("take no visibility"), "{errs:?}");
        let errs = errors(quote! { #[cfg(unix)] derive Id = Debug; });
        assert!(
            errs[0].contains("no attributes other than docs"),
            "{errs:?}"
        );
    }

    #[test]
    fn duplicate_names_and_cycles_are_rejected() {
        let errs = errors(quote! { derive A = Debug; derive A = Clone; });
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("`A` is declared twice"), "{errs:?}");
        let errs = errors(quote! { derive A = B; derive B = A, Debug; });
        assert_eq!(errs.len(), 1);
        assert!(errs[0].contains("cycle"), "{errs:?}");
        let errs = errors(quote! { derive A = A; });
        assert!(errs[0].contains("cycle"), "{errs:?}");
    }

    #[test]
    fn every_misuse_in_one_input_is_reported() {
        let errs = errors(quote! { derive A = ; mod m { derive Debug = Clone; pub derive B = ; } });
        assert_eq!(errs.len(), 4, "{errs:?}");
    }

    #[test]
    fn marker_is_gone_after_lowering() {
        let out = desugar(quote! { derive Id = Debug; }).unwrap();
        assert!(!out.contains("__rustx"), "{out}");
    }
}
