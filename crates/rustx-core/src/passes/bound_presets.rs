//! `bound Name = A + B;` presets → a trait with supertraits plus a blanket impl (problem 21).
//!
//! A token pass: `bound` items are not Rust, so they are rewritten before parsing.

use proc_macro2::TokenStream;

use crate::tokens::map_sequences;

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    // Identity until T6 implements this pass.
    map_sequences(tokens, &mut Ok)
}
