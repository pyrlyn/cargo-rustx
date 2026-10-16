//! Calls the code that `build.rs` desugared from `src/stats.rustx`.

use rustx_example_build_rs::{parse, summarize};

#[test]
fn the_desugared_rustx_code_runs() {
    let samples = parse("3 1 2").unwrap();
    let stats = summarize(&samples).unwrap();
    assert_eq!(stats.count, 3);
    assert!((stats.sum - 6.0).abs() < f64::EPSILON);
    assert!((stats.range() - 2.0).abs() < f64::EPSILON);
    assert!(summarize(&[]).is_none());
}

#[test]
fn a_throw_in_the_rustx_code_returns_its_error() {
    let err = parse("1 x").unwrap_err();
    assert_eq!(err.to_string(), "not a number: x");
}
