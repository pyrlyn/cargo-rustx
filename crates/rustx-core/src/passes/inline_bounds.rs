//! Inline parameter bounds `x: T: Clone + Debug` hoisted into generics / `where` (problem 03).
//!
//! A token pass makes the sugar parse; an AST pass moves the bounds where the fn header needs them.

use proc_macro2::TokenStream;

use crate::tokens::map_sequences;

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    // Identity until T5 implements this pass.
    map_sequences(tokens, &mut Ok)
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "identity until T5 implements the AST half of this pass"
)]
pub(super) fn lower(_file: &mut syn::File) -> syn::Result<()> {
    Ok(())
}
