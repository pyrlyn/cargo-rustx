//! `T throws` return types and tail `Ok(..)` (problem 10). Stub; the pass is being written.

use proc_macro2::TokenStream;

// The stubs cannot fail yet; the signatures are the ones every pass has.
#[allow(clippy::unnecessary_wraps)]
pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    Ok(tokens)
}

#[allow(clippy::unnecessary_wraps)]
pub(super) fn lower(_file: &mut syn::File) -> syn::Result<()> {
    Ok(())
}
