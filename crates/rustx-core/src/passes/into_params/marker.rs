//! Token half of `into T` parameters: `x: into T` in a fn parameter list becomes
//! `#[__rustx_into] x: T`.
//!
//! Parameter lists come from `tokens::params`. The first `:` outside `<..>` and outside `::`
//! ends the pattern; an `into` right after it is the sugar when a type can start after it. The
//! marker takes the span of that `into`, so a marker the AST half never reaches still points at
//! the user's token.

use proc_macro2::{Delimiter, Ident, Span, TokenStream, TokenTree};
use quote::quote_spanned;

use crate::tokens::angle::angle_depth;
use crate::tokens::params::map_fn_parameters;
use crate::tokens::punct::{is_punct, is_single_colon};
use crate::tokens::syntax::combine;

/// Name of the marker attribute; the AST half removes every one it finds.
pub(super) const MARKER: &str = "__rustx_into";

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    let mut errors = None;
    let out = map_fn_parameters(tokens, &mut |param| {
        Ok(rewrite_parameter(param, &mut errors))
    })?;
    errors.map_or(Ok(out), Err)
}

/// `x: into T` → `#[__rustx_into] x: T`; any other parameter is returned as written.
fn rewrite_parameter(param: &[TokenTree], errors: &mut Option<syn::Error>) -> Vec<TokenTree> {
    let Some(at) = into_at(param) else {
        return param.to_vec();
    };
    let span = param[at].span();
    let Some(next) = param.get(at + 1) else {
        let message = "expected a type after `into`, as in `name: into String`";
        combine(errors, syn::Error::new(span, message));
        return param.to_vec();
    };
    if !starts_type(next) {
        return param.to_vec();
    }
    let mut out: Vec<TokenTree> = marker(span).into_iter().collect();
    out.extend_from_slice(&param[..at]);
    out.extend_from_slice(&param[at + 1..]);
    out
}

/// Index of an `into` written right after the parameter's `:`.
fn into_at(param: &[TokenTree]) -> Option<usize> {
    let mut depth = 0;
    let colon = (0..param.len()).find(|&at| {
        depth = angle_depth(param, at, depth);
        depth == 0 && is_single_colon(param, at)
    })?;
    match param.get(colon + 1)? {
        // `r#into` prints with its `r#`, so it never matches and stays a plain type name.
        TokenTree::Ident(word) if word == "into" => Some(colon + 1),
        _ => None,
    }
}

/// Whether `next` can start the type of `into T`.
///
/// `into::m::T`, `into<T>` and `into!()` are valid Rust naming a type, module or macro called
/// `into`, so `::`, `<` and `!` keep the parameter as written.
fn starts_type(next: &TokenTree) -> bool {
    match next {
        TokenTree::Ident(_) => true,
        TokenTree::Group(group) => matches!(
            group.delimiter(),
            Delimiter::Parenthesis | Delimiter::Bracket
        ),
        punct => is_punct(punct, '&') || is_punct(punct, '*'),
    }
}

fn marker(span: Span) -> TokenStream {
    let name = Ident::new(MARKER, span);
    quote_spanned!(span=> #[#name])
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    fn compact(tokens: &TokenStream) -> String {
        tokens.to_string().split_whitespace().collect()
    }

    fn marks(sugar: &str) -> String {
        rewrite(sugar.parse().unwrap_or_default())
            .map_or_else(|err| err.to_string(), |out| compact(&out))
    }

    fn same(source: &str) -> String {
        compact(&source.parse().unwrap_or_default())
    }

    #[test]
    fn into_after_the_colon_becomes_a_marker() {
        assert_eq!(
            marks("fn f(x: into String, y: u8) {}"),
            same("fn f(#[__rustx_into] x: String, y: u8) {}")
        );
    }

    #[test]
    fn every_type_start_is_accepted() {
        assert_eq!(
            marks("fn f(a: into &str, b: into *const u8, c: into (u8, u8), d: into [u8; 2]) {}"),
            same(
                "fn f(#[__rustx_into] a: &str, #[__rustx_into] b: *const u8, \
                 #[__rustx_into] c: (u8, u8), #[__rustx_into] d: [u8; 2]) {}"
            )
        );
    }

    #[test]
    fn patterns_attributes_and_inline_bound_markers_stay_in_front() {
        assert_eq!(
            marks("fn f(#[__rustx_bound(Clone)] mut x: into Vec<T>) {}"),
            same("fn f(#[__rustx_into] #[__rustx_bound(Clone)] mut x: Vec<T>) {}")
        );
    }

    #[test]
    fn methods_in_nested_groups_are_rewritten() {
        assert_eq!(
            marks("impl S { fn m(&self, x: into T) { fn n(y: into U); } }"),
            same(
                "impl S { fn m(&self, #[__rustx_into] x: T) \
                 { fn n(#[__rustx_into] y: U); } }"
            )
        );
    }

    #[test]
    fn into_as_an_identifier_or_type_name_is_unchanged() {
        let valid = quote! {
            fn f(into: u8, x: r#into, p: into::T, g: into<u8>, m: into!(), q: <T as Into<U>>::A) {
                x.into();
                Into::into(x);
                let c = |y: into| y;
            }
            struct S { a: into }
        };
        assert_eq!(
            rewrite(valid.clone()).map(|out| out.to_string()).ok(),
            Some(valid.to_string())
        );
    }

    #[test]
    fn into_without_a_type_is_an_error() {
        assert_eq!(
            marks("fn f(x: into) {}"),
            "expected a type after `into`, as in `name: into String`"
        );
    }

    #[test]
    fn errors_from_several_parameters_are_reported_together() {
        let err = rewrite(quote! { fn f(a: into, b: into) {} })
            .err()
            .map(|err| err.into_iter().count());
        assert_eq!(err, Some(2));
    }
}
