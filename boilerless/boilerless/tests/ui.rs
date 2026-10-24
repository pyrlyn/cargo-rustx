//! Compile-fail cases: invalid declarations must produce clear `compile_error!`s.

#[test]
fn ui() {
    trybuild::TestCases::new().compile_fail("tests/ui/*.rs");
}
