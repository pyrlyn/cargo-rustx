//! Parameter defaults `port: u16 = 8080` → `Option<u16>` plus `unwrap_or` (problem 17).
//!
//! A token pass marks the parameter so it parses; an AST pass rewrites the type and the body.

use proc_macro2::TokenStream;

use crate::tokens::params::map_fn_parameters;

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    // Identity until T10.1 implements this pass.
    map_fn_parameters(tokens, &mut |param| Ok(param.to_vec()))
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "identity until T10.1 implements the AST half of this pass"
)]
pub(super) fn lower(_file: &mut syn::File) -> syn::Result<()> {
    Ok(())
}
