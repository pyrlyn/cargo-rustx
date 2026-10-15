//! `#[data]` structs: constructor, getters and an optional builder (problem 08).
//!
//! An AST pass: the struct is valid Rust, so it only needs the parsed fields.

#[expect(
    clippy::unnecessary_wraps,
    reason = "identity until T14 implements the AST half of this pass"
)]
pub(super) fn lower(_file: &mut syn::File) -> syn::Result<()> {
    Ok(())
}
