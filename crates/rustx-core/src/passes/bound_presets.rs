//! `bound Name = A + B;` presets → a trait with supertraits plus a blanket impl (problem 21).
//!
//! A token pass: `bound` items are not Rust, so they are rewritten before parsing. It runs
//! early so later passes only ever see ordinary traits.
//!
//! `[vis] bound Name = B1 + B2 + ..;` becomes the usual stable workaround for the unstable
//! `trait Name = ..;` aliases:
//!
//! ```text
//! [vis] trait Name: B1 + B2 + .. {}
//! impl<__RustxSelf: ?Sized + B1 + B2 + ..> Name for __RustxSelf {}
//! ```
//!
//! `Self` in the bounds (`Add<Output = Self>`) stays `Self` in the trait and becomes the impl's
//! type parameter in the impl. Supertraits are implied by `T: Name`, so code bounded by the
//! preset can use everything the bounds provide. Once native trait aliases are stable, the
//! expansion can switch to them without changing user code.
//!
//! # Detection
//!
//! `bound` is an ordinary identifier in Rust, so it is sugar only at item position and only
//! when an identifier other than `as` follows it, which valid Rust never has there:
//!
//! - Item position: the start of a token sequence, after `;`, after a `{ .. }` group or after
//!   an inner attribute, optionally followed by outer attributes and a visibility
//!   (`pub`, `pub(crate)`, `pub(in path)`).
//! - Everything else (`bound: u8`, `bound = 1`, `bound(x)`, `bound as u8`, `r#bound x`) passes
//!   through untouched.
//!
//! Once detected, the item must be `bound Name = <bounds>;` with at least one bound; generic
//! presets (`bound Num<Rhs> = ..`) are rejected because the problem file does not define them.
//!
//! # Attributes
//!
//! Outer attributes stay in front of the trait. `#[cfg(..)]` attributes are also copied onto
//! the blanket impl, because an impl of a trait that was configured away does not compile;
//! other attributes (docs, lints) describe the trait only.
//!
//! # Hygiene
//!
//! Generated tokens carry the span of the `bound` keyword so errors point at the preset. The
//! impl parameter uses `Span::mixed_site()`, but type parameters resolve at the call site even
//! there, so it also gets a reserved `__Rustx` name that user bounds cannot mean by accident.

use proc_macro2::{Delimiter, Ident, Span, TokenStream, TokenTree};
use quote::{ToTokens, quote_spanned};
use syn::Token;
use syn::parse::{ParseStream, Parser};
use syn::punctuated::Punctuated;

use crate::tokens::position::{at_statement_start, outer_attributes_start, visibility_start};
use crate::tokens::punct::{is_alone, is_punct, loosen_end};
use crate::tokens::syntax::{combine, introduced_span};
use crate::tokens::{map_sequences, map_sequences_reporting};

/// Name of the blanket impl's type parameter.
const PARAM: &str = "__RustxSelf";

pub(crate) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    map_sequences_reporting(tokens, rewrite_sequence)
}

fn rewrite_sequence(seq: &[TokenTree], errors: &mut Option<syn::Error>) -> Vec<TokenTree> {
    let mut out = Vec::with_capacity(seq.len());
    let mut i = 0;
    while i < seq.len() {
        if let Some(attrs) = preset_at(seq, i) {
            match expand(seq, attrs, i) {
                Ok((end, tokens)) => {
                    out.extend(tokens);
                    i = end;
                    continue;
                }
                Err(err) => combine(errors, err),
            }
        }
        out.push(seq[i].clone());
        i += 1;
    }
    out
}

/// If `seq[i]` is a `bound` preset keyword, returns where its outer attributes start.
fn preset_at(seq: &[TokenTree], i: usize) -> Option<usize> {
    let TokenTree::Ident(keyword) = &seq[i] else {
        return None;
    };
    if keyword != "bound" {
        return None;
    }
    match seq.get(i + 1)? {
        TokenTree::Ident(name) if name != "as" => {}
        _ => return None,
    }
    let attrs = outer_attributes_start(seq, visibility_start(seq, i));
    at_statement_start(&seq[..attrs]).then_some(attrs)
}

/// Expands the preset whose `bound` keyword is at `i`; returns the index after its `;`.
///
/// The attributes and visibility from `attrs` to `i` are already in the output; they are only
/// read here to copy `#[cfg(..)]` onto the impl.
fn expand(seq: &[TokenTree], attrs: usize, i: usize) -> syn::Result<(usize, TokenStream)> {
    let keyword = seq[i].span();
    let name = &seq[i + 1];
    let eq = match seq.get(i + 2) {
        Some(eq) if is_alone(eq, '=') => eq,
        Some(TokenTree::Punct(p)) if p.as_char() == '<' => {
            return Err(syn::Error::new(
                p.span(),
                "`bound` presets take no generic parameters; write a generic trait with a \
                 blanket impl instead",
            ));
        }
        next => {
            return Err(syn::Error::new(
                next.map_or(name.span(), TokenTree::span),
                "expected `=` after the preset name, as in `bound Name = A + B;`",
            ));
        }
    };
    let body = &seq[i + 3..];
    if body.first().is_none_or(|tree| is_punct(tree, ';')) {
        return Err(syn::Error::new(
            eq.span(),
            "expected at least one bound after `=`, as in `bound Name = A + B;`",
        ));
    }
    let semi = body.iter().position(|tree| is_punct(tree, ';'));
    // The `;` is parsed too, so a trailing `+` or a stray token is reported on a user token
    // instead of at an end of input that has no span of its own.
    let parse = |input: ParseStream| {
        Punctuated::<syn::TypeParamBound, Token![+]>::parse_separated_nonempty(input)?;
        if input.is_empty() {
            return Ok(false);
        }
        input
            .parse::<Token![;]>()
            .map_err(|_| input.error("expected `;` after the bound list"))?;
        Ok(true)
    };
    let end = semi.map_or(body.len(), |semi| semi + 1);
    let terminated = parse.parse2(body[..end].iter().cloned().collect())?;
    let Some(semi) = semi.filter(|_| terminated) else {
        // The list ran to the end of the sequence; its last token is the closest user token.
        return Err(syn::Error::new(
            body.last().map_or(eq.span(), TokenTree::span),
            "expected `;` after the bound list",
        ));
    };

    // The written tokens, not a re-print of the parsed bounds, so rustc errors in a bound
    // point at exactly what the user typed.
    let bounds: TokenStream = body[..semi].iter().cloned().collect();
    let impl_bounds = loosen_end(replace_self(bounds.clone())?);
    let param = param_ident(keyword);
    let trait_keyword = Ident::new("trait", keyword);
    let cfgs = cfg_attributes(&seq[attrs..i]);
    let tokens = quote_spanned! {keyword=>
        #trait_keyword #name: #bounds {}
        #(#cfgs)*
        impl<#param: ?::core::marker::Sized + #impl_bounds> #name for #param {}
    };
    Ok((i + 3 + semi + 1, tokens))
}

fn param_ident(located_at: Span) -> Ident {
    Ident::new(PARAM, introduced_span(located_at))
}

/// Replaces every `Self` with the impl parameter, keeping the position of each `Self`.
fn replace_self(tokens: TokenStream) -> syn::Result<TokenStream> {
    map_sequences(tokens, &mut |seq| {
        Ok(seq
            .into_iter()
            .map(|tree| match tree {
                TokenTree::Ident(ident) if ident == "Self" => {
                    TokenTree::Ident(param_ident(ident.span()))
                }
                other => other,
            })
            .collect())
    })
}

/// The `#[cfg(..)]` attributes among the outer attributes in `prefix`.
fn cfg_attributes(prefix: &[TokenTree]) -> Vec<TokenStream> {
    prefix
        .windows(2)
        .filter_map(|pair| match pair {
            [hash, TokenTree::Group(g)]
                if is_punct(hash, '#')
                    && g.delimiter() == Delimiter::Bracket
                    && matches!(g.stream().into_iter().next(), Some(TokenTree::Ident(i)) if i == "cfg") =>
            {
                Some(pair.iter().map(ToTokens::to_token_stream).collect())
            }
            _ => None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    /// Both sides re-printed through `syn`, so token spacing does not matter.
    fn expands_to(sugar: TokenStream, plain: TokenStream) {
        let normalize = |tokens: TokenStream| {
            syn::parse2::<syn::File>(tokens)
                .unwrap()
                .into_token_stream()
                .to_string()
        };
        assert_eq!(normalize(rewrite(sugar).unwrap()), normalize(plain));
    }

    fn unchanged(valid: &TokenStream) {
        assert_eq!(
            rewrite(valid.clone()).unwrap().to_string(),
            valid.to_string()
        );
    }

    fn error(sugar: TokenStream) -> String {
        rewrite(sugar).unwrap_err().to_string()
    }

    #[test]
    fn preset_becomes_trait_and_blanket_impl() {
        expands_to(
            quote! { bound Key = Clone + Debug + Eq + Hash + Send + Sync + 'static; },
            quote! {
                trait Key: Clone + Debug + Eq + Hash + Send + Sync + 'static {}
                impl<__RustxSelf: ?::core::marker::Sized + Clone + Debug + Eq + Hash + Send + Sync + 'static>
                    Key for __RustxSelf {}
            },
        );
    }

    #[test]
    fn self_stays_in_the_trait_and_becomes_the_parameter_in_the_impl() {
        expands_to(
            quote! { bound Num = Copy + Add<Output = Self> + Fn(Self) -> Vec<Self>; },
            quote! {
                trait Num: Copy + Add<Output = Self> + Fn(Self) -> Vec<Self> {}
                impl<__RustxSelf: ?::core::marker::Sized + Copy + Add<Output = __RustxSelf>
                    + Fn(__RustxSelf) -> Vec<__RustxSelf>> Num for __RustxSelf {}
            },
        );
    }

    #[test]
    fn visibility_and_attributes_are_kept_on_the_trait() {
        expands_to(
            quote! {
                /// Docs.
                pub bound A = Clone;
                pub(crate) bound B = Copy;
                #[allow(dead_code)] pub(in crate::m) bound C = Send;
            },
            quote! {
                /// Docs.
                pub trait A: Clone {}
                impl<__RustxSelf: ?::core::marker::Sized + Clone> A for __RustxSelf {}
                pub(crate) trait B: Copy {}
                impl<__RustxSelf: ?::core::marker::Sized + Copy> B for __RustxSelf {}
                #[allow(dead_code)] pub(in crate::m) trait C: Send {}
                impl<__RustxSelf: ?::core::marker::Sized + Send> C for __RustxSelf {}
            },
        );
    }

    #[test]
    fn cfg_attributes_also_guard_the_blanket_impl() {
        expands_to(
            quote! { #[doc = "x"] #[cfg(feature = "f")] bound A = Clone; },
            quote! {
                #[doc = "x"] #[cfg(feature = "f")] trait A: Clone {}
                #[cfg(feature = "f")] impl<__RustxSelf: ?::core::marker::Sized + Clone> A for __RustxSelf {}
            },
        );
    }

    #[test]
    fn presets_inside_modules_bodies_and_after_items_are_rewritten() {
        expands_to(
            quote! {
                #![allow(unused)]
                bound A = Clone;
                mod m { use std::fmt::Debug; bound B = Debug; }
                fn f() { bound C = Send; }
                struct S {} bound D = Sync;
            },
            quote! {
                #![allow(unused)]
                trait A: Clone {}
                impl<__RustxSelf: ?::core::marker::Sized + Clone> A for __RustxSelf {}
                mod m {
                    use std::fmt::Debug;
                    trait B: Debug {}
                    impl<__RustxSelf: ?::core::marker::Sized + Debug> B for __RustxSelf {}
                }
                fn f() {
                    trait C: Send {}
                    impl<__RustxSelf: ?::core::marker::Sized + Send> C for __RustxSelf {}
                }
                struct S {}
                trait D: Sync {}
                impl<__RustxSelf: ?::core::marker::Sized + Sync> D for __RustxSelf {}
            },
        );
    }

    #[test]
    fn bound_as_an_identifier_passes_through() {
        unchanged(&quote! {
            struct S { bound: u8, pub bound2: u8 }
            fn bound(bound: u8) -> u8 { bound }
            fn g(s: S) {
                let bound = 1;
                bound = 2;
                bound += 1;
                bound == 2;
                bound.f();
                bound(1);
                bound as u8;
                bound;
                s.bound;
                S { bound: 1 };
                S { bound };
                bound!(x);
                let r#bound = 0;
                match bound { bound => bound }
            }
            #[serde(bound = "T: Clone", bound(serialize = "T: Clone"))]
            struct T;
            macro_rules! m { (bound $x:ident) => {} }
        });
    }

    #[test]
    fn bound_outside_item_position_is_not_a_preset() {
        unchanged(&quote! { let a = bound K = Clone; f(x, bound K = Clone;); });
    }

    #[test]
    fn missing_equals_is_rejected() {
        for input in [
            quote! { bound Key Clone; },
            quote! { bound Key; },
            quote! { bound Key },
            quote! { bound Key == Clone; },
        ] {
            let err = error(input.clone());
            assert!(
                err.starts_with("expected `=` after the preset name"),
                "{input}: {err}"
            );
        }
    }

    #[test]
    fn generic_presets_are_rejected() {
        let err = error(quote! { bound Num<Rhs> = Add<Rhs>; });
        assert!(err.contains("take no generic parameters"), "{err}");
    }

    #[test]
    fn empty_bound_list_is_rejected() {
        let err = error(quote! { bound Key = ; });
        assert!(err.starts_with("expected at least one bound"), "{err}");
    }

    #[test]
    fn missing_semicolon_is_rejected() {
        let err = error(quote! { bound Key = Clone + Debug });
        assert_eq!(err, "expected `;` after the bound list");
        let err = error(quote! { bound Key = Clone + Debug fn f() {} use a::b; });
        assert_eq!(err, "expected `;` after the bound list");
        let err = error(quote! { bound Key = Clone + Debug fn f() {} });
        assert_eq!(err, "expected `;` after the bound list");
    }

    #[test]
    fn trailing_plus_is_rejected() {
        assert!(rewrite(quote! { bound Key = Clone + ; }).is_err());
    }

    #[test]
    fn every_misuse_in_one_input_is_reported() {
        let err = rewrite(quote! { bound A = ; mod m { bound B<T> = Clone; } }).unwrap_err();
        assert_eq!(err.into_iter().count(), 2);
    }
}
