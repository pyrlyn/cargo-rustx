//! `#[delegate(Trait)]` fields: a trait impl forwarding every method to the field (problem 07).
//!
//! An AST pass: the struct is valid Rust; method lists come from a built-in table of std traits
//! or from traits declared in the same input.

#[expect(
    clippy::unnecessary_wraps,
    reason = "identity until T16 implements the AST half of this pass"
)]
pub(super) fn lower(_file: &mut syn::File) -> syn::Result<()> {
    Ok(())
}
