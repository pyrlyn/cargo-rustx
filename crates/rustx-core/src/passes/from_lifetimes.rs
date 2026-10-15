//! Return lifetimes `-> &from(xs) T`, including `iter T` with `from` (problems 01, 09).
//!
//! A token pass makes the sugar parse; an AST pass adds the lifetime to the generics and parameters.

use proc_macro2::TokenStream;

use crate::tokens::map_sequences;

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    // Identity until T7 implements this pass.
    map_sequences(tokens, &mut Ok)
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "identity until T7 implements the AST half of this pass"
)]
pub(super) fn lower(_file: &mut syn::File) -> syn::Result<()> {
    Ok(())
}
