//! `rustx-cli fmt`: formats a `.rustx` file with rustfmt and keeps its sugar.
//!
//! rustfmt parses only Rust, so every sugar spelling is first replaced by a placeholder that is
//! valid Rust ([`find`], [`mapped`]); after rustfmt the sugar comes back. The layout is then the
//! one rustfmt gives ordinary Rust, and comments survive because rustfmt keeps them.
//!
//! Formatting must never change meaning: the result is accepted only when it expands to exactly
//! what the original expands to.

mod find;
mod mapped;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use proc_macro2::TokenStream;
use rustx_build::Kind;

use mapped::Mapped;

/// Formats `path` in place, or with `check` only reports whether it would change.
pub(crate) fn run(path: &Path, check: bool) -> Result<(), String> {
    let io = |err: std::io::Error| format!("{}: {err}", path.display());
    let source = std::fs::read_to_string(path).map_err(io)?;
    let formatted = format(path, &source)?;
    if formatted == source {
        Ok(())
    } else if check {
        let message = "not formatted; `rustx-cli fmt` rewrites it";
        Err(format!("{}: {message}", path.display()))
    } else {
        std::fs::write(path, formatted).map_err(io)
    }
}

/// The formatted text of `source`, read from `path`.
fn format(path: &Path, source: &str) -> Result<String, String> {
    let at = |(offset, message): (usize, String)| located(path, source, offset, &message);
    if Kind::of(path) != Kind::Sugar {
        let message =
            "only `.rustx` files can be formatted; `sugar!` bodies in Rust files cannot yet";
        return Err(format!("{}: {message}", path.display()));
    }
    let render = |err| rustx_build::render(path, source, err);
    let expanded = rustx_build::expand_source(source, Kind::Sugar).map_err(render)?;
    let tokens: TokenStream = syn::parse_str(source).map_err(render)?;
    let mut file = rustx_core::try_expand(tokens.clone()).map_err(render)?;
    let mapped = Mapped::new(source, find::find(source, tokens, &mut file));
    let formatted = rustfmt(&mapped, at)?;
    let restored = mapped.restore(&formatted).map_err(at)?;
    let expands_the_same =
        rustx_build::expand_source(&restored, Kind::Sugar).ok() == Some(expanded);
    if !expands_the_same {
        let message = "formatting would change what the file expands to, so it is left as it is";
        return Err(format!("{}: {message}", path.display()));
    }
    Ok(restored)
}

/// rustfmt's output for the mapped text, or why there is none, located with `at`.
const NOT_FOUND: &str =
    "error: `rustfmt` was not found; `rustup component add rustfmt` installs it";

fn rustfmt(mapped: &Mapped, at: impl Fn((usize, String)) -> String) -> Result<String, String> {
    let run = |err: std::io::Error| match err.kind() {
        std::io::ErrorKind::NotFound => NOT_FOUND.to_owned(),
        _ => format!("error: cannot run `rustfmt`: {err}"),
    };
    let mut child = Command::new("rustfmt")
        // Reordered imports or modules expand to reordered tokens, which the check against the
        // original expansion would reject.
        .args(["--edition", "2024", "--emit", "stdout"])
        .args(["--config", "reorder_imports=false,reorder_modules=false"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(run)?;
    // rustfmt reads all of stdin before it writes, so writing first cannot deadlock.
    if let Some(mut stdin) = child.stdin.take() {
        stdin.write_all(mapped.text.as_bytes()).map_err(run)?;
    }
    let output = child.wait_with_output().map_err(run)?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(at(rejected(mapped, &stderr)));
    }
    String::from_utf8(output.stdout).map_err(|err| format!("error: rustfmt: {err}"))
}

/// rustfmt's first error, at the source offset its position comes from.
fn rejected(mapped: &Mapped, stderr: &str) -> (usize, String) {
    let message = stderr.lines().find_map(|line| line.strip_prefix("error: "));
    let position = stderr.lines().find_map(|line| {
        let (line, column) = line
            .trim_start()
            .strip_prefix("--> <stdin>:")?
            .split_once(':')?;
        Some((line.parse::<usize>().ok()?, column.parse::<usize>().ok()?))
    });
    let offset = position.map_or(0, |(line, column)| {
        let text = &mapped.text;
        let start: usize = text
            .split_inclusive('\n')
            .take(line.saturating_sub(1))
            .map(str::len)
            .sum();
        let in_line = text[start..]
            .chars()
            .take(column.saturating_sub(1))
            .map(char::len_utf8)
            .sum::<usize>();
        mapped.source_offset(start + in_line)
    });
    let message = message.unwrap_or(stderr.trim());
    let message = format!(
        "rustfmt cannot parse this, probably sugar `rustx-cli fmt` does not handle yet: {message}"
    );
    (offset, message)
}

/// `path:line:column: message`, with 1-based line and column like `rustx_build::render`.
fn located(path: &Path, source: &str, offset: usize, message: &str) -> String {
    let before = &source[..offset.min(source.len())];
    let line = before.matches('\n').count() + 1;
    let column = before.chars().rev().take_while(|&c| c != '\n').count() + 1;
    format!("{}:{line}:{column}: {message}", path.display())
}
