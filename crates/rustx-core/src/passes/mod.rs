//! Registry of desugaring passes and the order they run in.
//!
//! One module per sugar item keeps passes independent; only this file knows the order.
//! Token passes run first, in list order, because their output must parse as Rust.
//! AST passes then run on the parsed file; all of them run so errors are reported together.

mod bound_presets;
mod from_lifetimes;
mod inline_bounds;
mod prefix_types;
mod turbofish;
mod var;

use proc_macro2::TokenStream;

use crate::tokens::syntax::combine;

type TokenPass = fn(TokenStream) -> syn::Result<TokenStream>;
type AstPass = fn(&mut syn::File) -> syn::Result<()>;

// `bound` presets first so later passes see ordinary traits; inline bounds and `from` lifetimes
// before `prefix_types`, so `iter T` can pick up a `from` lifetime instead of `'_`.
const TOKEN_PASSES: &[TokenPass] = &[
    var::rewrite,
    bound_presets::rewrite,
    inline_bounds::rewrite,
    from_lifetimes::rewrite,
    prefix_types::rewrite,
    turbofish::rewrite,
];

const AST_PASSES: &[AstPass] = &[inline_bounds::lower, from_lifetimes::lower];

pub(crate) fn rewrite_tokens(mut tokens: TokenStream) -> syn::Result<TokenStream> {
    for pass in TOKEN_PASSES {
        tokens = pass(tokens)?;
    }
    Ok(tokens)
}

pub(crate) fn rewrite_ast(file: &mut syn::File) -> syn::Result<()> {
    let mut errors: Option<syn::Error> = None;
    for pass in AST_PASSES {
        if let Err(err) = pass(file) {
            combine(&mut errors, err);
        }
    }
    errors.map_or(Ok(()), Err)
}
