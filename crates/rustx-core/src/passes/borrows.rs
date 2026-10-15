//! `#[borrows]` types: one lifetime added to the type, its fields and its `impl` headers (problem 19).
//!
//! An AST pass: references without a lifetime in a struct still parse, so the rewrite happens after
//! parsing, before `from_lifetimes` resolves `from(Self)` to the type's lifetime.

#[expect(
    clippy::unnecessary_wraps,
    reason = "identity until T8 implements the AST half of this pass"
)]
pub(super) fn lower(_file: &mut syn::File) -> syn::Result<()> {
    Ok(())
}
