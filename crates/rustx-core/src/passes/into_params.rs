//! `name: into T` parameters → `impl Into<T>` plus a conversion at the top of the body (problem 15).
//!
//! A token pass marks the parameter so it parses; an AST pass rewrites the type and the body.

use proc_macro2::TokenStream;

use crate::tokens::params::map_fn_parameters;

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    // Identity until T9 implements this pass.
    map_fn_parameters(tokens, &mut |param| Ok(param.to_vec()))
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "identity until T9 implements the AST half of this pass"
)]
pub(super) fn lower(_file: &mut syn::File) -> syn::Result<()> {
    Ok(())
}
