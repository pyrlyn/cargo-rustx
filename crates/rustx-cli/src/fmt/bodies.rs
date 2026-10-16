//! The `sugar! { .. }` bodies of a Rust file, formatted one by one like `.rustx` sources.
//!
//! Only the bodies change: the rest of the file is the user's Rust, which plain rustfmt formats
//! when they ask for it. Each body is formatted one level deeper than the line its invocation
//! starts on, and is accepted only when it expands to what it expanded to before.

use std::ops::Range;
use std::path::Path;

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use rustx_build::Kind;

use super::{format_sugar, located, same_expansion};

/// `source`, a Rust file read from `path`, with every `sugar!` body formatted.
pub(super) fn format(path: &Path, source: &str) -> Result<String, String> {
    let tokens: TokenStream =
        syn::parse_str(source).map_err(|err| rustx_build::render(path, source, err))?;
    let mut bodies = Vec::new();
    find(tokens, &mut bodies);
    let mut out = source.to_owned();
    for (invocation, body) in bodies.into_iter().rev() {
        let line = source[..invocation].rfind('\n').map_or(0, |at| at + 1);
        let indent: String = source[line..invocation]
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect();
        let formatted = format_body(path, source, body.clone(), &indent)?;
        out.replace_range(body, &formatted);
    }
    Ok(out)
}

/// The text between the braces of the body at `body`, formatted at `indent` plus one level.
fn format_body(
    path: &Path,
    source: &str,
    body: Range<usize>,
    indent: &str,
) -> Result<String, String> {
    if source[body.clone()].trim().is_empty() {
        return Ok(source[body].to_owned());
    }
    // Blanks in place of the text before the body keep every offset, line and column the same
    // as in the file, so errors and rustfmt's positions point into the file.
    let padding: String = source[..body.start]
        .chars()
        .map(|c| {
            if c == '\n' {
                "\n".to_owned()
            } else {
                " ".repeat(c.len_utf8())
            }
        })
        .collect();
    let text = padding + &source[body.clone()];
    let width: usize = indent.chars().map(|c| if c == '\t' { 4 } else { 1 }).sum();
    let (expanded, formatted) = format_sugar(path, source, &text, body.start, width / 4 + 1)?;
    let after = rustx_build::expand_source(&formatted, Kind::Sugar).ok();
    if !after.is_some_and(|after| same_expansion(&expanded, &after)) {
        let message = "formatting would change what this `sugar!` body expands to, so the file \
                       is left as it is";
        return Err(located(path, source, body.start, message));
    }
    Ok(format!("\n{formatted}{indent}"))
}

/// Each `sugar! { .. }` or `rustx::sugar! { .. }` in `tokens`: where the invocation starts and
/// the byte range between its braces. Bodies are not searched, since formatting one formats
/// what it holds.
fn find(tokens: TokenStream, bodies: &mut Vec<(usize, Range<usize>)>) {
    let seq: Vec<TokenTree> = tokens.into_iter().collect();
    for (i, tree) in seq.iter().enumerate() {
        let TokenTree::Group(group) = tree else {
            continue;
        };
        match sugar_path(&seq, i) {
            Some(start) if group.delimiter() == Delimiter::Brace => {
                let open = group.span_open().byte_range().end;
                bodies.push((start, open..group.span_close().byte_range().start));
            }
            _ => find(group.stream(), bodies),
        }
    }
}

/// Where the path of a `sugar!` call starts when the group at `i` is its body: `sugar!` or
/// `rustx::sugar!`, the paths the expander resolves.
fn sugar_path(seq: &[TokenTree], i: usize) -> Option<usize> {
    let at = |k: usize| i.checked_sub(k).and_then(|k| seq.get(k));
    let punct =
        |k: usize, ch: char| matches!(at(k), Some(TokenTree::Punct(p)) if p.as_char() == ch);
    let named = |k: usize, name: &str| matches!(at(k), Some(TokenTree::Ident(n)) if n == name);
    if !punct(1, '!') || !named(2, "sugar") {
        return None;
    }
    let start = |k: usize| at(k).map(|tree| tree.span().byte_range().start);
    if !punct(3, ':') {
        return start(2);
    }
    let rooted = punct(4, ':') && named(5, "rustx") && !punct(6, ':');
    rooted.then(|| start(5)).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bodies(source: &str) -> Vec<&str> {
        let mut found = Vec::new();
        find(source.parse().unwrap(), &mut found);
        found.into_iter().map(|(_, body)| &source[body]).collect()
    }

    #[test]
    fn sugar_and_rustx_sugar_bodies_are_found_where_they_are_called() {
        let source = "rustx::sugar! { a } fn f() { sugar! { b } } x::sugar! { c } sugar!(d);";
        assert_eq!(bodies(source), [" a ", " b "]);
    }
}
