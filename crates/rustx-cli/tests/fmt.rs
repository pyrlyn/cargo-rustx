//! Runs `rustx-cli fmt` on copies of the fixtures, so the checked-in files never change.
//!
//! Formatting needs `rustfmt` on `PATH`; without it the tests that format print a note and pass.

// `allow-unwrap-in-tests` only covers `#[test]` fns; the helpers below are test code as well.
#![allow(clippy::unwrap_used)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Cases with sugar that T31.2 maps (presets, inline bounds, parameter and call defaults, named
/// arguments, capture lists), or that do not expand at all.
const NOT_YET: &[&str] = &[
    "bound_presets",
    "call_defaults",
    "capture_lists",
    "derive_presets",
    "inline_bounds",
    "inline_bounds_known_type",
    "into_params",
    "named_args",
    "param_defaults",
    "prefix_types_tilde_without_trait",
    "shared",
];

fn rustx_cli(args: &[&str], path: &Path) -> Output {
    Command::new(env!("CARGO_BIN_EXE_rustx-cli"))
        .args(args)
        .arg(path)
        .output()
        .unwrap()
}

fn rustfmt_missing() -> bool {
    let found = Command::new("rustfmt").arg("--version").output();
    let missing = !found.is_ok_and(|out| out.status.success());
    if missing {
        eprintln!("note: `rustfmt` is not on PATH, so this test checks nothing");
    }
    missing
}

/// A fresh copy of `source` in a directory of its own, named after the test.
fn copy(test: &str, source: &Path) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("fmt")
        .join(test);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    let path = dir.join(source.file_name().unwrap());
    fs::copy(source, &path).unwrap();
    path
}

fn expand(path: &Path) -> Output {
    rustx_cli(&["expand"], path)
}

fn comments(text: &str) -> Vec<&str> {
    text.lines()
        .filter_map(|line| line.find("//").map(|at| line[at..].trim_end()))
        .collect()
}

#[test]
fn a_messy_file_gets_the_layout_rustfmt_gives_rust_and_keeps_its_comments() {
    if rustfmt_missing() {
        return;
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fmt_messy.rustx");
    let path = copy("messy", &source);
    let before = fs::read_to_string(&path).unwrap();
    let out = rustx_cli(&["fmt"], &path);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(out.stdout, b"");
    let after = fs::read_to_string(&path).unwrap();
    insta::assert_snapshot!("fmt_messy_rustx", after);
    assert_eq!(comments(&after), comments(&before));
    assert!(after.contains("/* Prefix types and values. */"));
    assert_eq!(expand(&path).stdout, expand(&source).stdout);
}

#[test]
fn check_reports_an_unformatted_file_and_writes_nothing() {
    if rustfmt_missing() {
        return;
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fmt_messy.rustx");
    let path = copy("check", &source);
    let out = rustx_cli(&["fmt", "--check"], &path);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert_eq!(
        stderr,
        format!(
            "{}: not formatted; `rustx-cli fmt` rewrites it\n",
            path.display()
        )
    );
    assert_eq!(fs::read(&path).unwrap(), fs::read(&source).unwrap());
    assert!(rustx_cli(&["fmt"], &path).status.success());
    assert!(rustx_cli(&["fmt", "--check"], &path).status.success());
}

#[test]
fn every_case_formats_once_for_good_and_expands_as_before() {
    if rustfmt_missing() {
        return;
    }
    let cases = Path::new(env!("CARGO_MANIFEST_DIR")).join("../rustx-core/tests/cases");
    let mut formatted = 0;
    for entry in fs::read_dir(cases).unwrap() {
        let source = entry.unwrap().path();
        if source.extension().is_none_or(|ext| ext != "rustx") {
            continue;
        }
        let name = source.file_stem().unwrap().to_str().unwrap().to_owned();
        let path = copy(&format!("case_{name}"), &source);
        let before = fs::read_to_string(&path).unwrap();
        let out = rustx_cli(&["fmt"], &path);
        let stderr = String::from_utf8_lossy(&out.stderr);
        if NOT_YET.contains(&name.as_str()) {
            assert_eq!(
                out.status.code(),
                Some(1),
                "{name} formatted; drop it from NOT_YET"
            );
            let at = format!("{}:", path.display());
            assert!(
                stderr.starts_with(&at) && stderr[at.len()..].contains(": "),
                "{stderr}"
            );
            assert_eq!(fs::read_to_string(&path).unwrap(), before);
            continue;
        }
        assert!(out.status.success(), "{name}: {stderr}");
        let check = rustx_cli(&["fmt", "--check"], &path);
        assert!(
            check.status.success(),
            "{name} changes again on a second run"
        );
        let after = fs::read_to_string(&path).unwrap();
        assert_eq!(comments(&after), comments(&before), "{name}");
        assert_eq!(expand(&path).stdout, expand(&source).stdout, "{name}");
        formatted += 1;
    }
    assert!(formatted >= 19, "only {formatted} cases formatted");
}

#[test]
fn sugar_without_a_placeholder_is_refused_at_its_position() {
    if rustfmt_missing() {
        return;
    }
    let source =
        Path::new(env!("CARGO_MANIFEST_DIR")).join("../rustx-core/tests/cases/named_args.rustx");
    let path = copy("refused", &source);
    let out = rustx_cli(&["fmt"], &path);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    let expected = format!(
        "{}:2:42: rustfmt cannot parse this, probably sugar",
        path.display()
    );
    assert!(stderr.starts_with(&expected), "{stderr}");
}

#[test]
fn a_rust_file_is_refused() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/server.rs");
    let out = rustx_cli(&["fmt"], &path);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert!(
        stderr.ends_with(
            ": only `.rustx` files can be formatted; `sugar!` bodies in Rust files cannot yet\n"
        ),
        "{stderr}"
    );
}

#[test]
fn a_missing_rustfmt_is_named_with_how_to_install_it() {
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/fmt_messy.rustx");
    let path = copy("no_rustfmt", &source);
    let out = Command::new(env!("CARGO_BIN_EXE_rustx-cli"))
        .args(["fmt"])
        .arg(&path)
        .env("PATH", "")
        .output()
        .unwrap();
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    assert_eq!(
        stderr,
        "error: `rustfmt` was not found; `rustup component add rustfmt` installs it\n"
    );
    assert_eq!(fs::read(&path).unwrap(), fs::read(&source).unwrap());
}
