//! `#[newtype(deref, display, from_str, validate = check)]` on a one-field tuple struct: one impl
//! per listed trait, plus `new` (problem 20).
//!
//! An AST pass: the struct is valid Rust, and nothing is generated that the attribute does not
//! name.

#[expect(
    clippy::unnecessary_wraps,
    reason = "identity until T17 implements the AST half of this pass"
)]
pub(super) fn lower(_file: &mut syn::File) -> syn::Result<()> {
    Ok(())
}
