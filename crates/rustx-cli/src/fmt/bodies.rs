//! The `sugar! { .. }` bodies of a Rust file, formatted one by one like `.rustx` sources.
//!
//! Only the bodies change: the rest of the file is the user's Rust, which plain rustfmt formats
//! when they ask for it. Each body is formatted one level deeper than the line its invocation
//! starts on, and is accepted only when it expands to what it expanded to before.

use std::ops::Range;
use std::path::Path;

use rustx_build::Kind;
use syn::spanned::Spanned;
use syn::visit_mut::VisitMut;

use super::{format_sugar, located, same_expansion};

/// `source`, a Rust file read from `path`, with every `sugar!` body formatted.
pub(super) fn format(path: &Path, source: &str) -> Result<String, String> {
    let mut file = parse(source).map_err(|err| rustx_build::render(path, source, err))?;
    let mut bodies = Vec::new();
    Bodies(&mut bodies).visit_file_mut(&mut file);
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

/// `parse_file` strips the BOM and shebang, but edits need offsets in the original source.
fn parse(source: &str) -> syn::Result<syn::File> {
    let file = syn::parse_file(source)?;
    let preamble = usize::from(source.starts_with('\u{feff}')) * '\u{feff}'.len_utf8()
        + file.shebang.as_ref().map_or(0, String::len);
    if preamble == 0 {
        Ok(file)
    } else {
        syn::parse_str(&pad_before(source, preamble))
    }
}

/// Byte-sized blanks keep both UTF-8 offsets and line numbers of the code after a prefix.
fn pad_before(source: &str, from: usize) -> String {
    let padding: String = source[..from]
        .chars()
        .map(|c| {
            if c == '\n' {
                "\n".to_owned()
            } else {
                " ".repeat(c.len_utf8())
            }
        })
        .collect();
    padding + &source[from..]
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
    let text = pad_before(&source[..body.end], body.start);
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

/// Only AST macro calls are executable here; tokens inside other macros stay opaque.
struct Bodies<'a>(&'a mut Vec<(usize, Range<usize>)>);

impl VisitMut for Bodies<'_> {
    fn visit_macro_mut(&mut self, mac: &mut syn::Macro) {
        if rustx_build::is_sugar(mac) {
            let span = mac.delimiter.span();
            self.0.push((
                mac.path.span().byte_range().start,
                span.open().byte_range().end..span.close().byte_range().start,
            ));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bodies(source: &str) -> Vec<&str> {
        let mut found = Vec::new();
        Bodies(&mut found).visit_file_mut(&mut parse(source).unwrap());
        found.into_iter().map(|(_, body)| &source[body]).collect()
    }

    #[test]
    fn sugar_and_rustx_sugar_bodies_are_found_where_they_are_called() {
        let source = "rustx::sugar! { a } fn f() { sugar! { b } } x::sugar! { c } sugar!(d); ::rustx::sugar![e];";
        assert_eq!(bodies(source), [" a ", " b ", "d", "e"]);
    }

    #[test]
    fn source_preambles_and_unicode_keep_body_offsets() {
        let source =
            "\u{feff}#!/usr/bin/env rust-script\n// café\n::rustx::sugar! { fn café() {} }";
        assert_eq!(bodies(source), [" fn café() {} "]);
    }

    #[test]
    fn macro_definitions_and_foreign_macro_inputs_are_opaque() {
        let source = "macro_rules! m { () => { sugar! { not Rust } }; } fn f() { inspect!(sugar! { not Rust }); }";
        assert_eq!(bodies(source), [] as [&str; 0]);
    }
}
