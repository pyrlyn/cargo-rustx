//! Token half of parameter defaults: `x: T = e` in a fn parameter list becomes
//! `#[__rustx_default(e)] x: T`.
//!
//! Parameter lists come from `tokens::params`. In a parameter, the first `=` outside `<..>` that
//! is not half of `==` or `=>` starts the default, which runs to the comma that ends the
//! parameter. No valid parameter has such an `=`, so valid Rust is never touched. Every
//! generated token takes the span of that `=`.

use proc_macro2::{Ident, Span, TokenStream, TokenTree};
use quote::quote_spanned;

use crate::tokens::angle::angle_depth;
use crate::tokens::params::map_fn_parameters;
use crate::tokens::punct::{is_joint, is_punct, loosen_end};

/// Name of the marker attribute; the AST half removes every one it finds.
pub(crate) const MARKER: &str = "__rustx_default";

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    map_fn_parameters(tokens, &mut rewrite_parameter)
}

/// `x: T = e` → `#[__rustx_default(e)] x: T`; any other parameter is returned as written.
fn rewrite_parameter(param: &[TokenTree]) -> syn::Result<Vec<TokenTree>> {
    let mut depth = 0;
    for at in 0..param.len() {
        depth = angle_depth(param, at, depth);
        if depth > 0 || !is_default_eq(param, at) {
            continue;
        }
        let eq = param[at].span();
        let default = &param[at + 1..];
        if default.is_empty() {
            return Err(syn::Error::new(
                eq,
                "expected a default value after `=`, as in `port: u16 = 8080`",
            ));
        }
        // The shared walker splits parameters at commas outside `<..>`, so a bare `<` in the
        // default (`a < b`) swallows the next parameter; it shows up here as a loose comma.
        if default.iter().any(|tree| is_punct(tree, ',')) {
            let at = default.iter().find(|tree| is_punct(tree, '<'));
            return Err(syn::Error::new(
                at.map_or(eq, TokenTree::span),
                "a `<` in a parameter default must be inside parentheses, as in \
                 `ok: bool = (a < b)`",
            ));
        }
        let mut out: Vec<TokenTree> = marker(eq, default).into_iter().collect();
        // The type may end in a `>` glued to the `=` (`Vec<u8>= ..`); it must not stay glued.
        out.extend(loosen_end(param[..at].iter().cloned()));
        return Ok(out);
    }
    Ok(param.to_vec())
}

/// Whether the token at `at` is an `=` that is neither half of `==` nor the head of `=>`.
///
/// Not `is_alone`: `x: i32 =-1` glues the `=` to the `-`, yet it is still a default.
fn is_default_eq(param: &[TokenTree], at: usize) -> bool {
    let glued_to = |next: char| {
        is_joint(&param[at], '=') && param.get(at + 1).is_some_and(|tree| is_punct(tree, next))
    };
    is_punct(&param[at], '=')
        && !glued_to('=')
        && !glued_to('>')
        && !at
            .checked_sub(1)
            .is_some_and(|before| is_joint(&param[before], '='))
}

fn marker(span: Span, default: &[TokenTree]) -> TokenStream {
    let default: TokenStream = default.iter().cloned().collect();
    let name = Ident::new(MARKER, span);
    quote_spanned!(span=> #[#name(#default)])
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
    fn a_default_becomes_a_marker_before_the_parameter() {
        assert_eq!(
            marks("fn f(host: &str, port: u16 = 5432, t: u64 = 5_000) {}"),
            same(
                "fn f(host: &str, #[__rustx_default(5432)] port: u16, \
                 #[__rustx_default(5_000)] t: u64) {}"
            )
        );
    }

    #[test]
    fn commas_inside_the_default_groups_stay_in_the_default() {
        assert_eq!(
            marks("fn f(p: (u8, u8) = (1, 2), v: Vec<u8> = vec![1, 2], m: M = m(a, b)) {}"),
            same(
                "fn f(#[__rustx_default((1, 2))] p: (u8, u8), \
                 #[__rustx_default(vec![1, 2])] v: Vec<u8>, \
                 #[__rustx_default(m(a, b))] m: M) {}"
            )
        );
    }

    #[test]
    fn an_eq_inside_angle_brackets_is_an_associated_type_not_a_default() {
        assert_eq!(
            marks("fn f(i: impl Iterator<Item = u8>, n: usize = 3) {}"),
            same("fn f(i: impl Iterator<Item = u8>, #[__rustx_default(3)] n: usize) {}")
        );
    }

    #[test]
    fn an_eq_glued_to_the_default_or_to_the_type_still_starts_it() {
        assert_eq!(
            marks("fn f(x: i32 =-1, v: Vec<u8>= Vec::new()) {}"),
            same(
                "fn f(#[__rustx_default(-1)] x: i32, \
                 #[__rustx_default(Vec::new())] v: Vec<u8>) {}"
            )
        );
    }

    #[test]
    fn methods_in_nested_groups_are_rewritten() {
        assert_eq!(
            marks("impl S { fn m(&self, x: u8 = 1) { fn n(y: u8 = 2) {} } }"),
            same(
                "impl S { fn m(&self, #[__rustx_default(1)] x: u8) \
                 { fn n(#[__rustx_default(2)] y: u8) {} } }"
            )
        );
    }

    #[test]
    fn valid_rust_with_eq_signs_is_unchanged() {
        let valid = quote! {
            fn f<const N: usize = 3, T = u8>(i: impl Iterator<Item = u8>, a: [u8; N]) -> bool {
                let x: u8 = 1;
                let c = |y: u8| y == x;
                a == [0; N]
            }
            struct S { a: u8 }
            type F = fn(u8) -> u8;
        };
        assert_eq!(
            rewrite(valid.clone()).map(|out| out.to_string()).ok(),
            Some(valid.to_string())
        );
    }

    #[test]
    fn an_eq_without_a_value_is_an_error() {
        assert_eq!(
            marks("fn f(x: u8 =) {}"),
            "expected a default value after `=`, as in `port: u16 = 8080`"
        );
    }

    #[test]
    fn a_bare_less_than_in_a_default_is_an_error() {
        assert!(marks("fn f(ok: bool = a < b, y: u8) {}").starts_with("a `<` in a parameter"));
        assert_eq!(
            marks("fn f(ok: bool = (a < b), y: u8) {}"),
            same("fn f(#[__rustx_default((a < b))] ok: bool, y: u8) {}")
        );
    }
}
