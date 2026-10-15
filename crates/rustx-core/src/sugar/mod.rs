//! The reverse of [`crate::try_expand`]: rewrites plain Rust into sugar where a pass would have
//! generated exactly that Rust (T19).
//!
//! Rules find generated shapes in the parsed file and return text edits as byte ranges of the
//! original source, so everything they do not touch (comments, layout) survives byte for byte.
//! The driver never trusts a rule: the edited text must expand back to the original tokens, and
//! an edit for which it does not is dropped. Tokens are compared with paths to std items written
//! in any spelling the file gives one meaning (`std_paths`), since passes emit full paths.

mod std_paths;
mod turbofish;
mod var;

use std::ops::Range;

use proc_macro2::{Delimiter, Span, TokenStream, TokenTree};
use quote::ToTokens;

/// One reverse rule per sugar item; it inverts what that item's pass generates.
pub(crate) trait ReverseRule {
    /// Edits turning the shapes this rule recognises in `file`, parsed from `source`, into sugar.
    /// They may overlap other rules' edits; the driver keeps the first of overlapping edits.
    fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit>;
}

/// Registered rules, in priority order for overlapping edits.
// `var` edits `let mut` and `turbofish` edits a `::` inside an expression, so their edits never
// overlap and their order does not matter.
const RULES: &[&dyn ReverseRule] = &[&var::LetMut, &turbofish::Turbofish];

/// Replaces the bytes `range` of the source with `text`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Edit {
    pub(crate) range: Range<usize>,
    pub(crate) text: String,
}

impl Edit {
    /// Replaces the source from the start of `first` to the end of `last`.
    pub(crate) fn between(first: Span, last: Span, text: &str) -> Self {
        Self {
            range: first.byte_range().start..last.byte_range().end,
            text: text.to_owned(),
        }
    }
}

const BYTE_ORDER_MARK: char = '\u{feff}';

/// Converts a plain Rust file into the text of a `.rustx` file (a `sugar!` body) that expands
/// back to the same tokens. Code no rule recognises comes back byte for byte.
///
/// # Errors
///
/// Returns the parse error when `source` is not a Rust file, or an error at the first token
/// that expansion would already change as written (such as `var x` inside a macro call), since
/// such a file means something else once it is read as sugar.
pub fn sugar(source: &str) -> syn::Result<String> {
    convert(source, RULES)
}

fn convert(source: &str, rules: &[&dyn ReverseRule]) -> syn::Result<String> {
    // The lexer skips a byte order mark, so offsets are taken from the text after it.
    let (bom, text) = match source.strip_prefix(BYTE_ORDER_MARK) {
        Some(rest) => (&source[..BYTE_ORDER_MARK.len_utf8()], rest),
        None => ("", source),
    };
    let file: syn::File = syn::parse_str(text)?;
    let original = file.to_token_stream();
    let expanded = crate::try_expand(text.parse()?)?.into_token_stream();
    if expanded.to_string() != original.to_string() {
        return Err(syn::Error::new(
            first_difference(original, expanded),
            "expansion already changes this code as written, so it would mean something else as \
             sugar",
        ));
    }

    let paths = std_paths::StdPaths::of(&file);
    let expected = paths.normalise(original);
    let round_trips = |sugar: &str| {
        sugar
            .parse::<TokenStream>()
            .ok()
            .and_then(|tokens| crate::try_expand(tokens).ok())
            .is_some_and(|file| paths.normalise(file.into_token_stream()) == expected)
    };
    let edits = non_overlapping(text, rules.iter().flat_map(|rule| rule.edits(&file, text)));
    let accepted = if round_trips(&apply(text, &edits)) {
        edits
    } else {
        // Some edit changes the meaning; keep each edit only if it does not, together with the
        // ones kept before it, so one bad edit does not cost the good ones.
        let mut kept = Vec::new();
        for edit in edits {
            kept.push(edit);
            if !round_trips(&apply(text, &kept)) {
                kept.pop();
            }
        }
        kept
    };
    Ok(format!("{bom}{}", apply(text, &accepted)))
}

/// Sorts edits by position and drops each one that overlaps an edit kept before it, or that is
/// not a range of `text` at all, so applying them cannot slice off a character.
fn non_overlapping(text: &str, edits: impl Iterator<Item = Edit>) -> Vec<Edit> {
    let mut edits: Vec<Edit> = edits
        .filter(|edit| text.get(edit.range.clone()).is_some())
        .collect();
    // Stable, so of two edits at one position the earlier rule wins.
    edits.sort_by_key(|edit| edit.range.start);
    let mut kept: Vec<Edit> = Vec::with_capacity(edits.len());
    for edit in edits {
        if kept
            .last()
            .is_none_or(|last| last.range.end <= edit.range.start)
        {
            kept.push(edit);
        }
    }
    kept
}

/// `text` with sorted, non-overlapping `edits` applied.
fn apply(text: &str, edits: &[Edit]) -> String {
    let mut out = String::with_capacity(text.len());
    let mut at = 0;
    for edit in edits {
        out.push_str(&text[at..edit.range.start]);
        out.push_str(&edit.text);
        at = edit.range.end;
    }
    out.push_str(&text[at..]);
    out
}

/// The span of the first token of `original` that `expanded` does not have.
fn first_difference(original: TokenStream, expanded: TokenStream) -> Span {
    let (mut a, mut b) = (Vec::new(), Vec::new());
    leaves(original, &mut a);
    leaves(expanded, &mut b);
    a.iter()
        .enumerate()
        .find(|(i, (ours, _))| b.get(*i).is_none_or(|(theirs, _)| theirs != ours))
        .map(|(_, leaf)| leaf)
        .or(a.last())
        .map_or_else(Span::call_site, |(_, span)| *span)
}

/// Every token of `tokens` with its span, delimiters included, in source order.
fn leaves(tokens: TokenStream, out: &mut Vec<(String, Span)>) {
    for tree in tokens {
        match tree {
            TokenTree::Group(group) => {
                let (open, close) = match group.delimiter() {
                    Delimiter::Parenthesis => ("(", ")"),
                    Delimiter::Brace => ("{", "}"),
                    Delimiter::Bracket => ("[", "]"),
                    Delimiter::None => ("", ""),
                };
                out.push((open.to_owned(), group.span_open()));
                leaves(group.stream(), out);
                out.push((close.to_owned(), group.span_close()));
            }
            leaf => out.push((leaf.to_string(), leaf.span())),
        }
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// The round-trip property every rule must hold: `plain` converts to `expected`, and
    /// expanding that gives back the tokens of `plain`, std paths compared by the item they name.
    /// Checked here on its own rather than trusting the driver's check, so a driver bug cannot
    /// hide a rule bug.
    pub(crate) fn assert_sugar(plain: &str, expected: &str) {
        let out = sugar(plain).unwrap();
        assert_eq!(out, expected);
        let original = syn::parse_file(plain).unwrap();
        let paths = std_paths::StdPaths::of(&original);
        let expanded = crate::try_expand(out.parse().unwrap()).unwrap();
        assert_eq!(
            paths.normalise(expanded.into_token_stream()),
            paths.normalise(original.into_token_stream()),
            "`{out}` does not expand back to the input"
        );
    }

    /// Edits every occurrence of a word, whether or not that keeps the meaning.
    struct Replace(&'static str, &'static str);

    impl ReverseRule for Replace {
        fn edits(&self, _: &syn::File, source: &str) -> Vec<Edit> {
            source
                .match_indices(self.0)
                .map(|(at, word)| Edit {
                    range: at..at + word.len(),
                    text: self.1.to_owned(),
                })
                .collect()
        }
    }

    #[test]
    fn rust_without_a_generated_shape_comes_back_byte_for_byte() {
        let source = "// lead\n#![allow(unused)]\n\nfn f(var: u8)  ->u8 { let x = var; /* c */ x }\n\
                      struct S { var: i32 }\nfn g() { let (mut a, b) = (1, 2); a += b; }\n";
        assert_eq!(sugar(source).unwrap(), source);
    }

    #[test]
    fn an_edit_that_changes_the_tokens_is_dropped_and_the_others_kept() {
        let source = "fn f() { let a = 1; }\nfn g() { let b = 2; }\n";
        let out = convert(
            source,
            &[&Replace("f()", "h()"), &Replace("let b", "let mut b")],
        );
        assert_eq!(out.unwrap(), source);
        let out = convert(source, &[&Replace("let a = 1;", "let a = 1 ;")]);
        assert_eq!(
            out.unwrap(),
            "fn f() { let a = 1 ; }\nfn g() { let b = 2; }\n"
        );
    }

    #[test]
    fn of_overlapping_edits_the_earlier_rule_wins() {
        let source = "fn f() {}\n";
        let first = Replace("fn f", "fn  f");
        let second = Replace("fn", "fn\n");
        assert_eq!(convert(source, &[&first, &second]).unwrap(), "fn  f() {}\n");
        assert_eq!(
            convert(source, &[&second, &first]).unwrap(),
            "fn\n f() {}\n"
        );
    }

    #[test]
    fn an_edit_outside_the_source_or_inside_a_character_is_dropped() {
        let source = "fn f() { \"é\"; }\n";
        let edits = [
            Edit {
                range: 0..99,
                text: String::new(),
            },
            Edit {
                range: 11..12,
                text: String::new(),
            },
        ];
        assert_eq!(non_overlapping(source, edits.into_iter()), []);
    }

    #[test]
    fn a_byte_order_mark_is_kept_and_does_not_shift_edits() {
        assert_sugar(
            "\u{feff}fn f() { let mut x = 1; }",
            "\u{feff}fn f() { var x = 1; }",
        );
    }

    #[test]
    fn a_syntax_error_is_reported_at_its_position() {
        let err = sugar("fn f() {\n    let = 1;\n}\n").unwrap_err();
        assert_eq!(err.span().start().line, 2);
    }

    #[test]
    fn code_that_expansion_already_changes_is_an_error_at_that_code() {
        let err = sugar("fn f() {\n    m! { var x = 1; }\n}\n").unwrap_err();
        assert!(err.to_string().contains("as written"), "{err}");
        let start = err.span().start();
        assert_eq!((start.line, start.column), (2, 9));
    }
}
