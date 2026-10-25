//! Runs `rustx-cli fmt` on copies of the fixtures, so the checked-in files never change.
//!
//! Formatting needs `rustfmt` on `PATH`, supplied by the project toolchain.

// `allow-unwrap-in-tests` only covers `#[test]` fns; the helpers below are test code as well.
#![allow(clippy::unwrap_used)]

mod support;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Cases that do not expand, so there is nothing to check a formatted file against.
const NOT_YET: &[&str] = &[
    "inline_bounds_known_type",
    "prefix_types_tilde_without_trait",
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

/// Comparing executed expansions catches macro arms that token normalisation would hide.
fn execute(path: &Path) -> Vec<u8> {
    let expanded = expand(path);
    assert!(
        expanded.status.success(),
        "{}",
        String::from_utf8_lossy(&expanded.stderr)
    );
    support::compile_and_run(&expanded.stdout, path.parent().unwrap())
}

#[test]
fn formatting_preserves_macro_behavior() {
    for fixture in ["fmt_macro_comma.rustx", "fmt_foreign_macro.rs"] {
        let source = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(fixture);
        let path = copy(fixture, &source);
        let before = execute(&path);
        assert_eq!(before, b"1\n", "{fixture}");
        let out = rustx_cli(&["fmt"], &path);
        assert!(
            out.status.success(),
            "{fixture}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        assert_eq!(execute(&path), before, "{fixture}");
        if fixture == "fmt_foreign_macro.rs" {
            assert_eq!(fs::read(&path).unwrap(), fs::read(&source).unwrap());
        }
        assert!(
            rustx_cli(&["fmt", "--check"], &path).status.success(),
            "{fixture}"
        );
    }
}

/// The expansion of `path` after rustfmt, so a trailing comma rustfmt adds inside `vec![..]`,
/// which the expansion keeps as written, compares equal to the original.
fn expand_formatted(path: &Path) -> Vec<u8> {
    use std::io::Write;
    let expanded = expand(path).stdout;
    let mut child = Command::new("rustfmt")
        .args(["--edition", "2024", "--emit", "stdout"])
        .args(["--config", "reorder_imports=false,reorder_modules=false"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(&expanded).unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&expanded)
    );
    out.stdout
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
        assert_eq!(expand_formatted(&path), expand_formatted(&source), "{name}");
        formatted += 1;
    }
    assert!(formatted >= 30, "only {formatted} cases formatted");
}

#[test]
fn what_rustfmt_cannot_parse_is_refused_at_its_position() {
    if rustfmt_missing() {
        return;
    }
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join("fmt/refused");
    fs::create_dir_all(&dir).unwrap();
    // `gen` is a keyword of edition 2024, which rustfmt parses and `syn` does not know.
    let path = dir.join("refused.rustx");
    let source = "fn f() {\n    var x = 1;\n    let gen = x;\n}\n";
    fs::write(&path, source).unwrap();
    let out = rustx_cli(&["fmt"], &path);
    assert_eq!(out.status.code(), Some(1));
    let stderr = String::from_utf8(out.stderr).unwrap();
    let expected = format!("{}:3:9: rustfmt cannot parse this", path.display());
    assert!(stderr.starts_with(&expected), "{stderr}");
    assert_eq!(fs::read_to_string(&path).unwrap(), source);
}

#[test]
fn sugar_bodies_in_a_rust_file_are_formatted_and_the_rest_is_kept() {
    if rustfmt_missing() {
        return;
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/server.rs");
    let path = copy("rust_file", &source);
    let before = fs::read_to_string(&path).unwrap();
    let out = rustx_cli(&["fmt"], &path);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let after = fs::read_to_string(&path).unwrap();
    insta::assert_snapshot!("fmt_server_rs", after);
    assert!(rustx_cli(&["fmt", "--check"], &path).status.success());
    assert_eq!(comments(&after), comments(&before));
    let outside = |text: &str| -> Vec<String> {
        let first = text.find("rustx::sugar! {").unwrap();
        let tail = text.rfind("    println!").unwrap();
        vec![text[..first].to_owned(), text[tail..].to_owned()]
    };
    assert_eq!(outside(&after), outside(&before));
    assert_eq!(expand_formatted(&path), expand_formatted(&source));
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
