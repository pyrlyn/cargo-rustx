//! Token half of inline bounds: `x: T: B` in a fn parameter list becomes
//! `#[__rustx_bound(B)] x: T`.
//!
//! Parameter lists come from `tokens::params`. In a parameter, a second `:` outside `<..>` and
//! outside `::` starts the bounds, which run to the end of the parameter. Every generated token
//! takes the span of that `:`.

use proc_macro2::{Ident, Span, TokenStream, TokenTree};
use quote::quote_spanned;

use crate::tokens::angle::angle_depth;
use crate::tokens::params::map_fn_parameters;
use crate::tokens::punct::is_single_colon;

/// Name of the marker attribute; the AST half removes every one it finds.
pub(super) const MARKER: &str = "__rustx_bound";

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    map_fn_parameters(tokens, &mut rewrite_parameter)
}

/// `x: T: B` → `#[__rustx_bound(B)] x: T`; any other parameter is returned as written.
fn rewrite_parameter(param: &[TokenTree]) -> syn::Result<Vec<TokenTree>> {
    let mut depth = 0;
    let mut colons = 0;
    for at in 0..param.len() {
        depth = angle_depth(param, at, depth);
        if depth > 0 || !is_single_colon(param, at) {
            continue;
        }
        colons += 1;
        if colons < 2 {
            continue;
        }
        let colon = param[at].span();
        let bounds = &param[at + 1..];
        if bounds.is_empty() {
            return Err(syn::Error::new(
                colon,
                "expected bounds after `:`, as in `x: T: Clone`",
            ));
        }
        let mut out: Vec<TokenTree> = marker(colon, bounds).into_iter().collect();
        out.extend_from_slice(&param[..at]);
        return Ok(out);
    }
    Ok(param.to_vec())
}

fn marker(span: Span, bounds: &[TokenTree]) -> TokenStream {
    let bounds: TokenStream = bounds.iter().cloned().collect();
    let name = Ident::new(MARKER, span);
    quote_spanned!(span=> #[#name(#bounds)])
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
    fn a_second_colon_turns_the_bounds_into_a_marker() {
        assert_eq!(
            marks("fn f(x: T: Clone + Debug, y: u8) {}"),
            same("fn f(#[__rustx_bound(Clone + Debug)] x: T, y: u8) {}")
        );
    }

    #[test]
    fn commas_and_colons_inside_angle_brackets_stay_in_the_bounds() {
        assert_eq!(
            marks("fn f<A: Fn() -> u8>(m: M: Into<HashMap<K, V>>, i: I: Iterator<Item: Clone>) {}"),
            same(
                "fn f<A: Fn() -> u8>(#[__rustx_bound(Into<HashMap<K, V>>)] m: M, \
                 #[__rustx_bound(Iterator<Item: Clone>)] i: I) {}"
            )
        );
    }

    #[test]
    fn closures_returning_bounds_stay_whole() {
        assert_eq!(
            marks("fn f(g: F: FnOnce(&K) -> Result<V, E>) {}"),
            same("fn f(#[__rustx_bound(FnOnce(&K) -> Result<V, E>)] g: F) {}")
        );
    }

    #[test]
    fn methods_in_nested_groups_are_rewritten() {
        assert_eq!(
            marks("impl S { fn m(&self, x: T: Copy) { fn n(y: U: Send); } }"),
            same(
                "impl S { fn m(&self, #[__rustx_bound(Copy)] x: T) \
                 { fn n(#[__rustx_bound(Send)] y: U); } }"
            )
        );
    }

    #[test]
    fn valid_parameters_are_unchanged() {
        let valid = quote! {
            fn f(x: T, (a, b): (u8, u8), p: ::std::path::PathBuf, q: <T as Tr>::A, S { a, .. }: S) {}
            fn g<T: Iterator<Item: Clone>>(f: fn(x: u8) -> u8, i: impl Iterator<Item: Copy>) {}
            struct P { a: B, c: D }
            let c = |x: u8, y: Vec<u8>| x;
            s(a: b);
        };
        assert_eq!(
            rewrite(valid.clone()).map(|out| out.to_string()).ok(),
            Some(valid.to_string())
        );
    }

    #[test]
    fn a_colon_without_bounds_is_an_error() {
        assert_eq!(
            marks("fn f(x: T:) {}"),
            "expected bounds after `:`, as in `x: T: Clone`"
        );
    }
}
