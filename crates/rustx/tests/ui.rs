//! Compile tests: `pass/` must build and run through `sugar!`, `fail/` must fail with the
//! pinned `.stderr`, which checks that error spans point at the user's tokens.

#[test]
fn ui() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/pass/*.rs");
    cases.compile_fail("tests/fail/*.rs");
}
