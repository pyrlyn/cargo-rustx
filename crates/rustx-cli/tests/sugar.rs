//! Runs the built `rustx-cli sugar` on the fixtures in `tests/fixtures`.

// `allow-unwrap-in-tests` only covers `#[test]` fns; the helpers below are test code as well.
#![allow(clippy::unwrap_used)]

use std::process::{Command, Output};

/// Paths are given relative to the crate so the error lines are the same on every machine.
fn rustx_cli(args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rustx-cli"))
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .args(args)
        .output()
        .unwrap()
}

fn success(args: &[&str]) -> String {
    let out = rustx_cli(args);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert_eq!(stderr, "");
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn a_rust_file_is_printed_as_rustx_text_with_its_comments() {
    insta::assert_snapshot!(
        "sugar_counter_rs",
        success(&["sugar", "tests/fixtures/sugar_counter.rs"])
    );
}

#[test]
fn the_output_saved_as_rustx_expands_to_the_same_rust_as_the_input() {
    let converted = success(&["sugar", "tests/fixtures/sugar_counter.rs"]);
    let rustx = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("sugar_counter.rustx");
    std::fs::write(&rustx, converted).unwrap();
    assert_eq!(
        success(&["expand", rustx.to_str().unwrap()]),
        success(&["expand", "tests/fixtures/sugar_counter.rs"])
    );
}

#[test]
fn rc_values_written_with_short_std_names_become_rc_sugar() {
    let converted = success(&["sugar", "tests/fixtures/sugar_rc.rs"]);
    insta::assert_snapshot!("sugar_rc_rs", converted);
    // The expansion spells the paths in full, so it is not the input's text; the core tests
    // compare the tokens with std paths resolved.
    let rustx = std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("sugar_rc.rustx");
    std::fs::write(&rustx, converted).unwrap();
    let expanded = success(&["expand", rustx.to_str().unwrap()]);
    assert!(
        expanded.contains("::std::rc::Rc::new(::std::cell::RefCell::new(0))"),
        "{expanded}"
    );
}

#[test]
fn a_generated_impl_default_becomes_field_defaults() {
    let converted = success(&["sugar", "tests/fixtures/sugar_field_defaults.rs"]);
    insta::assert_snapshot!("sugar_field_defaults_rs", converted);
    let rustx =
        std::path::Path::new(env!("CARGO_TARGET_TMPDIR")).join("sugar_field_defaults.rustx");
    std::fs::write(&rustx, converted).unwrap();
    let expanded = success(&["expand", rustx.to_str().unwrap()]);
    assert!(
        expanded.contains("impl ::core::default::Default for Config"),
        "{expanded}"
    );
}

#[test]
fn a_file_without_a_sugar_shape_comes_back_byte_for_byte() {
    let path = "tests/fixtures/sugar_plain.rs";
    let source =
        std::fs::read_to_string(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path));
    assert_eq!(success(&["sugar", path]), source.unwrap());
}

#[test]
fn code_that_would_read_differently_as_sugar_is_an_error_at_its_position() {
    let out = rustx_cli(&["sugar", "tests/fixtures/sugar_macro_var.rs"]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(out.stdout, b"");
    assert_eq!(
        String::from_utf8(out.stderr).unwrap(),
        "tests/fixtures/sugar_macro_var.rs:2:10: expansion already changes this code as \
         written, so it would mean something else as sugar\n"
    );
}

#[test]
fn a_missing_file_is_reported_without_a_panic() {
    let out = rustx_cli(&["sugar", "tests/fixtures/missing.rs"]);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.starts_with("tests/fixtures/missing.rs: "),
        "{stderr}"
    );
    assert!(!stderr.contains("panicked"), "{stderr}");
}
