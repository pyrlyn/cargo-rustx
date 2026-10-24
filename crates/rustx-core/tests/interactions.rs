//! Regression boundaries between token passes and typed literal conversion.
use quote::ToTokens;

#[test]
fn capture_lists_do_not_split_closure_parameters_into_named_arguments() {
    rustx_core::try_expand(
        "fn h(f: impl Fn(u8, u8) -> u8) {} fn f() { let c = 1; h(move[c] |a: u8, b: u8| a+b+c); }"
            .parse()
            .unwrap(),
    )
    .unwrap();
}

#[test]
fn string_defaults_and_given_arguments_both_allocate_owned_strings() {
    let expanded = rustx_core::try_expand(
        "fn greet(name: String = \"admin\") {} fn f() { greet(\"ann\"); }"
            .parse()
            .unwrap(),
    )
    .unwrap()
    .into_token_stream()
    .to_string();
    assert!(
        expanded.contains("String :: from (\"admin\")"),
        "{expanded}"
    );
    assert!(expanded.contains("String :: from (\"ann\")"), "{expanded}");
    assert!(expanded.contains("unwrap_or_else"), "{expanded}");
}
