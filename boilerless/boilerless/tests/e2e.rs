//! End-to-end: each file in tests/e2e is a real boilerplate example taken through the
//! macro; it must compile and its `main()` asserts must pass.

#[test]
fn e2e() {
    trybuild::TestCases::new().pass("tests/e2e/*.rs");
}
