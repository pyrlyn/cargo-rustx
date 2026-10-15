//! `derive` presets: `derive Id = Debug, Clone;` expands inside every `#[derive(..)]` (problem 18).
//!
//! A token pass takes the preset items out, because `derive Name = ..;` is not Rust; an AST pass
//! expands the derive lists.

use proc_macro2::TokenStream;

#[expect(
    clippy::unnecessary_wraps,
    reason = "identity until T12 implements the token half of this pass"
)]
pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    Ok(tokens)
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "identity until T12 implements the AST half of this pass"
)]
pub(super) fn lower(_file: &mut syn::File) -> syn::Result<()> {
    Ok(())
}
