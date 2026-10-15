//! Implicit lossless `into()` under `#[widen]` (problem 13). Stub; the pass is being written.

// The stub cannot fail yet; the signature is the one every AST pass has.
#[allow(clippy::unnecessary_wraps)]
pub(super) fn lower(_file: &mut syn::File) -> syn::Result<()> {
    Ok(())
}
