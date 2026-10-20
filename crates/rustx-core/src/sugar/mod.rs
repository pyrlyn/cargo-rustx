//! The reverse of [`crate::try_expand`]: rewrites plain Rust into sugar where a pass would have
//! generated exactly that Rust (T19).
//!
//! Rules find generated shapes in the parsed file and return text edits as byte ranges of the
//! original source, so everything they do not touch (comments, layout) survives byte for byte.
//! The driver never trusts a rule: the edited text must expand back to the original tokens, and
//! an edit for which it does not is dropped. Tokens are compared with paths to std items written
//! in any spelling the file gives one meaning (`std_paths`), since passes emit full paths, and
//! without the trailing commas rustfmt adds to lists it breaks over lines (`commas`), since
//! passes write none: a comma before a closing delimiter, before the `>` of generics, or before
//! the body of a `where` list, but never inside the tokens a macro reads, nor the comma that
//! makes a one-element tuple.

mod bound_presets;
mod capture_lists;
mod commas;
mod delegate;
mod field_defaults;
mod from_display;
mod inline_bounds;
mod into_params;
mod item_lists;
mod newtype;
mod param_defaults;
mod prefix_types;
mod rc_borrows;
mod rc_values;
mod std_paths;
mod string_literals;
mod throws;
mod turbofish;
mod var;

pub use commas::{MacroTokens, without_trailing_commas};

use std::ops::Range;

use proc_macro2::{Delimiter, Span, TokenStream, TokenTree};
use quote::ToTokens;

/// One reverse rule per sugar item; it inverts what that item's pass generates.
pub(crate) trait ReverseRule {
    /// Edits turning the shapes this rule recognises in `file`, parsed from `source`, into sugar.
    /// They may overlap other rules' edits; the driver keeps the first of overlapping edits.
    fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit>;

    /// The edits in sets the driver keeps or drops as a whole, for a rule whose edits keep the
    /// meaning only together; by default each edit is a set of its own.
    fn edit_groups(&self, file: &syn::File, source: &str) -> Vec<Vec<Edit>> {
        self.edits(file, source)
            .into_iter()
            .map(|edit| vec![edit])
            .collect()
    }
}

/// Registered rules, in priority order for overlapping edits.
// `var` edits `let mut` and `turbofish` edits a `::` inside an expression, so their edits never
// overlap each other. An `rc` edit replaces its whole call, so an edit inside its value overlaps
// it and is dropped whatever the order: such a value keeps its plain spelling. Likewise an edit
// inside a default that `field_defaults` moves from an impl into its struct. `prefix_types` edits
// types and `rc_values` expressions, so their edits never overlap; a `prefix_types` edit inside an
// inline bounds signature, a parameter default's type or a `throws` return type overlaps that
// rule's edit and, listed first, wins. An inline bounds edit spans a signature from its generics
// to its `where`, so it wins any later rule's edit inside that span. A parameter default's
// statement removal starts at its body's `{`, so it wins over a `var` edit inside it. `throws`
// edits a return type and the bytes around a tail's value. An `into` edit starts in the
// signature, before any edit of the rebindings it removes, so it wins those too. A string literal
// edit replaces one `String::from(lit)` call, which holds no other rule's shape. A bound preset
// edit spans a trait and its blanket impl, so an earlier rule's edit inside its bounds wins.
// A capture list's edits leave out its captures and closure, so edits inside them never overlap.
// A newtype's edits start at its struct, before the impls they remove, so they win any edit
// inside those.
const RULES: &[&dyn ReverseRule] = &[
    &var::LetMut,
    &turbofish::Turbofish,
    &rc_values::RcValues,
    &field_defaults::FieldDefaults,
    &prefix_types::PrefixTypes,
    &inline_bounds::InlineBounds,
    &param_defaults::ParamDefaults,
    &throws::Throws,
    &into_params::IntoParams,
    &string_literals::StringLiterals,
    // Its `from` edit ends where another rule's edit of a variant type starts, so both are kept
    // and the driver's check keeps whichever expands back; its removal of the impls wins any
    // later edit inside them.
    &from_display::FromDisplay,
    &bound_presets::BoundPresets,
    &capture_lists::CaptureLists,
    // Its set inserts attributes at fields and removes the impls after the struct. Another rule's
    // edit of the struct that overlaps none of these, such as a field type, is kept with it. A set
    // starting earlier that overlaps one of its edits drops all of it; it wins any set starting
    // later, such as an edit inside the impls.
    &delegate::Delegate,
    &newtype::Newtypes,
];

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
    // A pass that rebuilds a list (call arguments) writes it without a trailing comma, which
    // changes nothing, so rustfmt's commas must not make every multi-line call an error.
    let written = without_trailing_commas(original.clone(), MacroTokens::Kept);
    let expanded = without_trailing_commas(expanded, MacroTokens::Kept);
    if expanded.to_string() != written.to_string() {
        return Err(syn::Error::new(
            first_difference(written, expanded),
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
    let groups = non_overlapping(
        text,
        rules.iter().flat_map(|rule| rule.edit_groups(&file, text)),
    );
    let accepted = if round_trips(&apply(text, &flatten(&groups))) {
        groups
    } else {
        // Some edit changes the meaning; keep each set of edits only if it does not, together
        // with the ones kept before it, so one bad set does not cost the good ones. A set is
        // never split: part of it may keep the meaning while not being what the rule meant.
        let mut kept = Vec::new();
        for group in groups {
            kept.push(group);
            if !round_trips(&apply(text, &flatten(&kept))) {
                kept.pop();
            }
        }
        kept
    };
    Ok(format!("{bom}{}", apply(text, &flatten(&accepted))))
}

/// Sorts sets of edits by their first position and drops each set with an edit that overlaps an
/// edit kept before it or of its own, or that is not a range of `text` at all, so applying them
/// cannot slice off a character.
fn non_overlapping(text: &str, groups: impl Iterator<Item = Vec<Edit>>) -> Vec<Vec<Edit>> {
    let mut groups: Vec<Vec<Edit>> = groups
        .map(|mut group| {
            group.sort_by_key(|edit| edit.range.start);
            group
        })
        .filter(|group| {
            group
                .iter()
                .all(|edit| text.get(edit.range.clone()).is_some())
                && group.windows(2).all(|pair| !overlap(&pair[0], &pair[1]))
        })
        .collect();
    // Stable, so of two sets at one position the earlier rule wins.
    groups.sort_by_key(|group| group.first().map(|edit| edit.range.start));
    let mut kept: Vec<Vec<Edit>> = Vec::with_capacity(groups.len());
    for group in groups {
        let free = group
            .iter()
            .all(|edit| kept.iter().flatten().all(|old| !overlap(old, edit)));
        if free {
            kept.push(group);
        }
    }
    kept
}

/// Whether `new` touches bytes `old`, kept before it, replaces. An insertion right where a
/// replacement starts goes before it when it was kept first.
fn overlap(old: &Edit, new: &Edit) -> bool {
    if old.range.start <= new.range.start {
        new.range.start < old.range.end
    } else {
        old.range.start < new.range.end
    }
}

/// The edits of non-overlapping sets in source order, as [`apply`] takes them.
fn flatten(groups: &[Vec<Edit>]) -> Vec<Edit> {
    let mut edits: Vec<Edit> = groups.iter().flatten().cloned().collect();
    edits.sort_by_key(|edit| edit.range.start);
    edits
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

/// The spans of the first and last token of `tokens`.
pub(super) fn bounds(tokens: TokenStream) -> Option<(Span, Span)> {
    let mut all = Vec::new();
    leaves(tokens, &mut all);
    Some((all.first()?.1, all.last()?.1))
}

/// The byte range of `tokens` in the source, from its first token to its last.
pub(super) fn span_range(tokens: TokenStream) -> Option<Range<usize>> {
    let (first, last) = bounds(tokens)?;
    Some(first.byte_range().start..last.byte_range().end)
}

/// Whether removing `text`, a run of whole tokens, would drop a comment, a doc comment included.
/// `//` or `/*` inside a string or char literal is no comment, so this lexes `text` rather than
/// searching it: the lexer drops comments, so a byte outside every token that is not whitespace
/// is one. Text that does not lex counts as a comment, so the caller keeps it.
pub(super) fn has_comment(text: &str) -> bool {
    let Ok(tokens) = text.parse::<TokenStream>() else {
        return true;
    };
    let mut all = Vec::new();
    leaves(tokens, &mut all);
    let mut covered = vec![false; text.len()];
    for (_, span) in all {
        let range = span.byte_range();
        let Some(written) = text.get(range.clone()) else {
            return true;
        };
        // A doc comment lexes into `#[doc = ".."]` tokens spanning the comment.
        if written.starts_with("//") || written.starts_with("/*") {
            return true;
        }
        covered[range].fill(true);
    }
    text.char_indices()
        .any(|(at, c)| !covered[at] && !c.is_whitespace())
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

    /// Edits every occurrence of each word, all of them as one set.
    struct Together(&'static [(&'static str, &'static str)]);

    impl ReverseRule for Together {
        fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit> {
            self.edit_groups(file, source).concat()
        }

        fn edit_groups(&self, file: &syn::File, source: &str) -> Vec<Vec<Edit>> {
            let edits = self
                .0
                .iter()
                .flat_map(|&(from, to)| Replace(from, to).edits(file, source));
            vec![edits.collect()]
        }
    }

    #[test]
    fn a_set_of_edits_is_kept_or_dropped_as_a_whole() {
        let source = "fn f() { let a = 1; }\nfn g() { let b = 2; }\n";
        // The first edit keeps the meaning on its own, the second does not: the set goes.
        let partial = Together(&[("let a = 1;", "let a = 1 ;"), ("f()", "h()")]);
        let out = convert(source, &[&partial, &Replace("let b = 2;", "let b = 2 ;")]);
        assert_eq!(
            out.unwrap(),
            "fn f() { let a = 1; }\nfn g() { let b = 2 ; }\n"
        );
        // One edit of the set overlaps an edit kept before it: the other goes with it.
        let late = Together(&[("fn f()", "fn f ()"), ("let b", "let  b")]);
        let out = convert(source, &[&Replace("fn f", "fn  f"), &late]);
        assert_eq!(
            out.unwrap(),
            "fn  f() { let a = 1; }\nfn g() { let b = 2; }\n"
        );
        let whole = Together(&[("let a = 1;", "let a = 1 ;"), ("let b", "let  b")]);
        assert_eq!(
            convert(source, &[&whole]).unwrap(),
            "fn f() { let a = 1 ; }\nfn g() { let  b = 2; }\n"
        );
    }

    #[test]
    fn a_set_whose_edits_keep_the_meaning_only_together_is_kept_one_at_a_time() {
        let source = "fn f() { let a = 1; let b = 2; }\n";
        // Either edit alone drops or repeats `let a`; together they only move a line break.
        let moved = Together(&[
            ("let a = 1; ", ""),
            ("let b = 2;", "let a = 1;\n    let b = 2;"),
        ]);
        // The renamed fn fails the whole-file check, so every set is checked on its own.
        let out = convert(source, &[&moved, &Replace("f()", "h()")]);
        assert_eq!(out.unwrap(), "fn f() { let a = 1;\n    let b = 2; }\n");
    }

    #[test]
    fn two_sets_of_edits_in_one_fn_both_apply_over_rustfmt_commas() {
        let source = "fn f<T>(\n    t: T,\n) -> u8\nwhere\n    T: Clone,\n{\n    let mut a = 1;\n    \
                      let mut b = 2;\n    a + b\n}\n";
        let expected = "fn f<T>(\n    t: T\n) -> u8\nwhere\n    T: Clone\n{\n    var a = 1;\n    \
                        var b = 2;\n    a + b\n}\n";
        // Each set edits two places, the sets interleave, and each drops one of rustfmt's
        // trailing commas, which the check must not count as a change.
        let first = Together(&[("t: T,\n)", "t: T\n)"), ("let mut a", "var a")]);
        let second = Together(&[("T: Clone,\n{", "T: Clone\n{"), ("let mut b", "var b")]);
        assert_eq!(convert(source, &[&first, &second]).unwrap(), expected);
        // A set between them that changes the return type sends the driver to its
        // one-at-a-time check; it goes whole, its harmless edit too, and both sets stay.
        let bad = Together(&[("-> u8", "-> i8"), ("a + b", "a+b")]);
        assert_eq!(convert(source, &[&first, &bad, &second]).unwrap(), expected);
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
        assert_eq!(
            non_overlapping(source, edits.into_iter().map(|edit| vec![edit])),
            Vec::<Vec<Edit>>::new()
        );
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

    #[test]
    fn comment_markers_inside_literals_are_no_comments() {
        assert!(!has_comment(
            " impl A { fn f() { g(\"http://x/*\", '/', b\"//\", r\"/*\") } }\n"
        ));
        assert!(!has_comment("  \n"));
        for commented in [
            "fn f() {} // why",
            "fn f() { /* why */ }",
            "/// why\nfn f() {}",
            "//! why",
            "fn f() { \"",
        ] {
            assert!(has_comment(commented), "{commented}");
        }
    }

    #[test]
    fn rustfmt_trailing_commas_neither_block_the_file_nor_the_check() {
        // The call-argument pass rewrites `g(1,)` without its comma, and passes write lists
        // without one; neither may stop a rustfmt-formatted file from converting.
        let source = "fn f<T>(\n    t: T,\n) -> u8\nwhere\n    T: Clone,\n{\n    let mut x = g(\n        \
                      1,\n    );\n    x\n}\n";
        assert_sugar(source, &source.replace("let mut", "var"));
        let out = convert(source, &[&Replace("t: T,\n)", "t: T\n)")]);
        assert_eq!(out.unwrap(), source.replace("t: T,\n)", "t: T\n)"));
    }
}
