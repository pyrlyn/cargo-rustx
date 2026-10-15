//! `from` variants `Io(from io::Error)` → `From` impls, and `#[display(..)]` → `Display` (problem 05).
//!
//! A token pass marks `from` in a variant, which does not parse; an AST pass generates the impls.

use proc_macro2::TokenStream;

#[expect(
    clippy::unnecessary_wraps,
    reason = "identity until T15 implements the token half of this pass"
)]
pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    Ok(tokens)
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "identity until T15 implements the AST half of this pass"
)]
pub(super) fn lower(_file: &mut syn::File) -> syn::Result<()> {
    Ok(())
}
