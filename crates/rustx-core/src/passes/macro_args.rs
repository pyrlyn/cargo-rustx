//! Macro bodies made of expressions, so AST passes can reach sugar inside `vec![..]`,
//! `format!(..)` and `assert_eq!(..)`, whose tokens `syn` leaves unparsed.
//!
//! Shared because every AST pass that rewrites expressions must also look into these bodies, and
//! must re-emit them without losing a `vec![x; n]` or a trailing comma.

use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Expr, Token};

/// A macro body made of expressions: `a, b, c` (`format!`, `assert_eq!`) or `value; count`
/// (`vec![x; n]`).
pub(crate) enum MacroArgs {
    List(Punctuated<Expr, Token![,]>),
    Repeat(Box<Expr>, Token![;], Box<Expr>),
}

impl MacroArgs {
    /// Every expression of the body, in source order.
    pub(crate) fn exprs_mut(&mut self) -> Vec<&mut Expr> {
        match self {
            Self::List(list) => list.iter_mut().collect(),
            Self::Repeat(value, _, count) => vec![&mut **value, &mut **count],
        }
    }
}

impl Parse for MacroArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let first: Expr = input.parse()?;
        if input.peek(Token![;]) {
            return Ok(Self::Repeat(
                Box::new(first),
                input.parse()?,
                input.parse()?,
            ));
        }
        let mut list = Punctuated::new();
        list.push_value(first);
        while !input.is_empty() {
            list.push_punct(input.parse()?);
            if input.is_empty() {
                break;
            }
            list.push_value(input.parse()?);
        }
        Ok(Self::List(list))
    }
}

impl ToTokens for MacroArgs {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::List(list) => list.to_tokens(tokens),
            Self::Repeat(value, semi, count) => {
                value.to_tokens(tokens);
                semi.to_tokens(tokens);
                count.to_tokens(tokens);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    fn reprint(body: TokenStream) -> (usize, String) {
        let mut args: MacroArgs = syn::parse2(body).unwrap();
        (args.exprs_mut().len(), args.into_token_stream().to_string())
    }

    #[test]
    fn lists_and_repeats_round_trip() {
        for (body, count) in [
            (quote!(a, f(b), c), 3),
            (quote!("{}", x,), 2),
            (quote!(S { a: 1 }; n + 1), 2),
        ] {
            assert_eq!(reprint(body.clone()), (count, body.to_string()));
        }
    }

    #[test]
    fn bodies_that_are_not_expressions_do_not_parse() {
        for body in [quote!(), quote!(x => y), quote!(a b)] {
            assert!(syn::parse2::<MacroArgs>(body).is_err());
        }
    }
}
