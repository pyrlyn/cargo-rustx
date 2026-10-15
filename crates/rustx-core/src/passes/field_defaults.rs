//! Field defaults `port: u16 = 8080` → `impl Default`, and `S { a: 1, .. }` → `..Default::default()`
//! (problem 17, fields).
//!
//! A token pass marks the defaults and fills the bare `..`, which do not parse; an AST pass
//! generates `impl Default`.

use proc_macro2::TokenStream;

#[expect(
    clippy::unnecessary_wraps,
    reason = "identity until T13 implements the token half of this pass"
)]
pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    Ok(tokens)
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "identity until T13 implements the AST half of this pass"
)]
pub(super) fn lower(_file: &mut syn::File) -> syn::Result<()> {
    Ok(())
}
