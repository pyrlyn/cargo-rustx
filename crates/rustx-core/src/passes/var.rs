//! Statement sugar `var x = ..` → `let mut x = ..` (problem 02).
//!
//! A token pass: `var x` is not a Rust statement, so it must be rewritten before `syn` parses.

use proc_macro2::TokenStream;

use crate::tokens::map_sequences;

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    // Identity until T2 implements this pass.
    map_sequences(tokens, &mut Ok)
}
