//! `rustx-cli fmt`: formats a `.rustx` file, or the `sugar!` bodies of a Rust file ([`bodies`]),
//! with rustfmt and keeps the sugar.
//!
//! rustfmt parses only Rust, so every sugar spelling is first replaced by a placeholder that is
//! valid Rust ([`find`], [`mapped`]); after rustfmt the sugar comes back. The layout is then the
//! one rustfmt gives ordinary Rust, and comments survive because rustfmt keeps them.
//!
//! Formatting must never change meaning: the result is accepted only when it expands to exactly
//! what the original expands to.

mod bodies;
mod find;
mod mapped;

use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

use proc_macro2::TokenStream;
use rustx_build::Kind;
use rustx_core::{MacroTokens, without_trailing_commas};

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
    if Kind::of(path) == Kind::Rust {
        return bodies::format(path, source);
    }
    let (expanded, restored) = format_sugar(path, source, source, 0, 0)?;
    let after = rustx_build::expand_source(&restored, Kind::Sugar).ok();
    if !after.is_some_and(|after| same_expansion(&expanded, &after)) {
        let message = "formatting would change what the file expands to, so it is left as it is";
        return Err(format!("{}: {message}", path.display()));
    }
    Ok(restored)
}

/// What opens each level of nesting around a `sugar!` body sent to rustfmt.
const NEST: &str = "mod rustx_fmt {\n";

/// The expansion of `text`, sugar whose byte offsets are those of the file `source`, and the
/// formatted text of `text[from..]`, nested `levels` deep.
///
/// The nesting makes rustfmt indent a body as deep as the file does, also the lines it cannot
/// format and keeps as written, so a second run changes nothing.
fn format_sugar(
    path: &Path,
    source: &str,
    text: &str,
    from: usize,
    levels: usize,
) -> Result<(String, String), String> {
    let at = |(offset, message): (usize, String)| located(path, source, offset, &message);
    let render = |err| rustx_build::render(path, source, err);
    let expanded = rustx_build::expand_source(text, Kind::Sugar).map_err(render)?;
    let tokens: TokenStream = syn::parse_str(text).map_err(render)?;
    let mut file = rustx_core::try_expand(tokens.clone()).map_err(render)?;
    let mapped = Mapped::new(text, find::find(text, tokens, &mut file));
    let open = NEST.repeat(levels);
    // rustfmt keeps a blank line it finds after the `{` of the nesting.
    let body = mapped.text[from..].trim_start();
    let start = mapped.text.len() - body.len();
    let nested = [open.as_str(), body, &"\n}".repeat(levels)].concat();
    let locate = |offset: usize| {
        let offset = (offset + start).saturating_sub(open.len()).max(start);
        mapped.source_offset(offset)
    };
    let formatted = rustfmt(&nested, locate, at)?;
    let moved = || {
        at((
            from,
            "rustfmt moved code out of this `sugar!` body".to_owned(),
        ))
    };
    let inner = unnest(&formatted, levels).ok_or_else(moved)?;
    let restored = mapped.restore(inner).map_err(at)?;
    Ok((expanded, restored))
}

/// `formatted` without the `levels` of nesting [`format_sugar`] put around it.
fn unnest(formatted: &str, levels: usize) -> Option<&str> {
    let mut inner = formatted;
    for _ in 0..levels {
        let (first, rest) = inner.split_once('\n')?;
        let close = rest.trim_end().strip_suffix('}')?;
        let line = close.rfind('\n').map_or(0, |at| at + 1);
        if first.trim() != NEST.trim() || !close[line..].trim().is_empty() {
            return None;
        }
        inner = &close[..line];
    }
    Some(inner)
}

/// Whether two expansions mean the same, up to trailing commas that mean nothing.
///
/// prettyplease settles trailing commas in syntax it knows, but prints macro arguments as they
/// are written, and rustfmt adds a trailing comma when it breaks `vec![..]` over lines, so here
/// they are ignored inside macro arguments too. A comma that makes a one-element tuple, `(x,)`,
/// is kept, because without it the meaning changes.
fn same_expansion(before: &str, after: &str) -> bool {
    let tokens = |text: &str| {
        text.parse::<TokenStream>()
            .ok()
            .map(|t| without_trailing_commas(t, MacroTokens::Ignored))
    };
    match (tokens(before), tokens(after)) {
        (Some(before), Some(after)) => before.to_string() == after.to_string(),
        _ => before == after,
    }
}

const NOT_FOUND: &str =
    "error: `rustfmt` was not found; `rustup component add rustfmt` installs it";

/// rustfmt's output for `text`, or why there is none, located with `at`.
fn rustfmt(
    text: &str,
    locate: impl Fn(usize) -> usize,
    at: impl Fn((usize, String)) -> String,
) -> Result<String, String> {
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
        stdin.write_all(text.as_bytes()).map_err(run)?;
    }
    let output = child.wait_with_output().map_err(run)?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        return Err(at(rejected(text, &stderr, locate)));
    }
    String::from_utf8(output.stdout).map_err(|err| format!("error: rustfmt: {err}"))
}

/// rustfmt's first error in `text`, at the source offset `locate` gives its position.
fn rejected(text: &str, stderr: &str, locate: impl Fn(usize) -> usize) -> (usize, String) {
    let message = stderr.lines().find_map(|line| line.strip_prefix("error: "));
    let position = stderr.lines().find_map(|line| {
        let (line, column) = line
            .trim_start()
            .strip_prefix("--> <stdin>:")?
            .split_once(':')?;
        Some((line.parse::<usize>().ok()?, column.parse::<usize>().ok()?))
    });
    let offset = position.map_or(0, |(line, column)| {
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
        locate(start + in_line)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn expansion(source: &str) -> String {
        rustx_build::expand_source(source, Kind::Sugar).unwrap()
    }

    #[test]
    fn a_trailing_comma_rustfmt_adds_to_vec_arguments_keeps_the_expansion() {
        let before = expansion("fn f() { let v: Vec<String> = vec![\"a\", \"b\"]; }");
        let after = expansion("fn f() { let v: Vec<String> = vec![\"a\", \"b\",]; }");
        assert_ne!(before, after);
        assert!(same_expansion(&before, &after));
    }

    #[test]
    fn a_comma_that_makes_a_tuple_or_any_other_token_still_differs() {
        let tuple = |source| expansion(&format!("fn f() {{ m!({source}); }}"));
        assert!(!same_expansion(&tuple("x"), &tuple("x,")));
        assert!(same_expansion(&tuple("x, y"), &tuple("x, y,")));
        assert!(!same_expansion(&tuple("x, y"), &tuple("x, z")));
    }
}
