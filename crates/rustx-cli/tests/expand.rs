//! Runs the built `rustx-cli` binary on the fixtures in `tests/fixtures`.

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

fn expand(path: &str) -> String {
    let out = rustx_cli(&["expand", path]);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "{stderr}");
    assert_eq!(stderr, "");
    String::from_utf8(out.stdout).unwrap()
}

fn expand_error(path: &str) -> String {
    let out = rustx_cli(&["expand", path]);
    assert_eq!(out.status.code(), Some(1));
    assert_eq!(out.stdout, b"");
    String::from_utf8(out.stderr).unwrap()
}

#[test]
fn a_rustx_file_is_desugared_as_a_whole() {
    insta::assert_snapshot!("stats_rustx", expand("tests/fixtures/stats.rustx"));
}

#[test]
fn sugar_in_a_rust_file_is_expanded_as_an_item_and_in_a_fn_body() {
    insta::assert_snapshot!("server_rs", expand("tests/fixtures/server.rs"));
}

#[test]
fn a_desugar_error_in_a_rustx_file_names_path_line_and_column() {
    assert_eq!(
        expand_error("tests/fixtures/tilde_without_trait.rustx"),
        "tests/fixtures/tilde_without_trait.rustx:1:22: \
         expected a trait after `~`, as in `~Trait` or `~Fn(T) -> U`\n"
    );
}

#[test]
fn a_desugar_error_inside_sugar_points_into_the_rust_file() {
    assert_eq!(
        expand_error("tests/fixtures/tilde_without_trait.rs"),
        "tests/fixtures/tilde_without_trait.rs:3:30: \
         expected a trait after `~`, as in `~Trait` or `~Fn(T) -> U`\n"
    );
}

#[test]
fn a_missing_file_is_reported_without_a_panic() {
    let stderr = expand_error("tests/fixtures/missing.rustx");
    assert!(
        stderr.starts_with("tests/fixtures/missing.rustx: "),
        "{stderr}"
    );
    assert!(!stderr.contains("panicked"), "{stderr}");
}

#[test]
fn a_missing_subcommand_is_a_usage_error() {
    let out = rustx_cli(&[]);
    assert_eq!(out.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&out.stderr).contains("expand"));
}
