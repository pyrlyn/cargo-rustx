//! Type-prefix sugar: `~Trait`, `iter T`, `rc T`, `weak T` (problems 04, 09, 16).
//!
//! A token pass: a keyword or `~` before a type is not valid Rust, so it is rewritten before parsing.

use proc_macro2::TokenStream;

use crate::tokens::map_sequences;

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    // Identity until T3 implements this pass.
    map_sequences(tokens, &mut Ok)
}
