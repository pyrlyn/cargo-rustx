//! `rc e` expressions (problem 16). Stub; the pass is being written.

use proc_macro2::TokenStream;

// The stub cannot fail yet; the signature is the one every token pass has.
#[allow(clippy::unnecessary_wraps)]
pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    Ok(tokens)
}
