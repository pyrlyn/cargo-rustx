//! Method generics without turbofish: `.m<T>()` → `.m::<T>()` (problem 06).
//!
//! A token pass: without `::` the call parses as comparisons, so it is rewritten before parsing.

use proc_macro2::TokenStream;

use crate::tokens::map_sequences;

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    // Identity until T4 implements this pass.
    map_sequences(tokens, &mut Ok)
}
