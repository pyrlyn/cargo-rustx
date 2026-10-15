//! Trailing commas the sugar check ignores (T19.11).
//!
//! rustfmt ends every list it breaks over lines with a comma (a `where` list, generics,
//! parameters, arguments, struct literals), while passes write lists without one, so a rule that
//! rewrites such a list would never expand back to the same tokens. In Rust's own syntax a
//! trailing comma means nothing, except the one that makes a one-element tuple, tuple type or
//! tuple pattern: `(x,)`. Inside a macro call or an attribute the macro reads the tokens, and a
//! `macro_rules` arm may match that comma, so there it counts unless the caller says otherwise.
//!
//! This only compares; no text is ever rewritten without a comma.

use proc_macro2::{Delimiter, Group, TokenStream, TokenTree};

use crate::tokens::angle::angle_depth;
use crate::tokens::punct::{is_joint, is_punct};

/// Whether [`without_trailing_commas`] also ignores trailing commas in the tokens a macro reads.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MacroTokens {
    /// Kept as written inside macro calls and attributes, since the macro may tell them apart.
    Kept,
    /// Ignored there as well, for comparing a formatter's output with its input: rustfmt adds a
    /// trailing comma to macro arguments it formats as Rust (`vec![..]`).
    Ignored,
}

/// Keywords after which a parenthesised group is a tuple rather than an argument or parameter
/// list.
const KEYWORDS: &[&str] = &[
    "as", "async", "await", "become", "box", "break", "const", "continue", "crate", "do", "dyn",
    "else", "enum", "extern", "false", "fn", "for", "gen", "if", "impl", "in", "let", "loop",
    "macro", "match", "mod", "move", "mut", "pub", "ref", "return", "self", "static", "struct",
    "super", "trait", "true", "try", "type", "unsafe", "use", "where", "while", "yield",
];

/// `tokens` without the trailing commas that cannot change their meaning: the last comma before
/// a closing delimiter (not the one of a one-element tuple), a comma before the `>` that closes
/// generics, and a comma before the body brace that ends a `where` list.
#[must_use]
pub fn without_trailing_commas(tokens: TokenStream, macros: MacroTokens) -> TokenStream {
    strip(tokens, Delimiter::None, true, macros)
}

/// `list` says a parenthesised group is an argument or parameter list, where a single comma
/// cannot be what makes a tuple.
fn strip(
    tokens: TokenStream,
    delimiter: Delimiter,
    list: bool,
    macros: MacroTokens,
) -> TokenStream {
    let seq: Vec<TokenTree> = tokens.into_iter().collect();
    let commas = seq.iter().filter(|tree| is_punct(tree, ',')).count();
    let mut out = Vec::with_capacity(seq.len());
    // Angle depth inside a `where` list, so a `{` in generic arguments does not end it.
    let mut in_where: Option<usize> = None;
    for (at, tree) in seq.iter().enumerate() {
        let next = seq.get(at + 1);
        in_where = match tree {
            TokenTree::Ident(ident) if ident == "where" => Some(0),
            TokenTree::Group(group)
                if group.delimiter() == Delimiter::Brace && in_where == Some(0) =>
            {
                None
            }
            _ => in_where.map(|depth| angle_depth(&seq, at, depth)),
        };
        match tree {
            TokenTree::Group(group)
                if !(macros == MacroTokens::Kept && read_by_macro(&seq, at)) =>
            {
                let list = group.delimiter() != Delimiter::Parenthesis || names_a_list(&seq, at);
                let stream = strip(group.stream(), group.delimiter(), list, macros);
                let mut fresh = Group::new(group.delimiter(), stream);
                fresh.set_span(group.span());
                out.push(TokenTree::Group(fresh));
                continue;
            }
            tree if is_punct(tree, ',') => {
                let closes = next.is_none() && delimiter != Delimiter::None && (list || commas > 1);
                let ends_generics = next.is_some_and(|next| is_punct(next, '>'));
                let ends_where = in_where == Some(0)
                    && matches!(next, Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Brace);
                if closes || ends_generics || ends_where {
                    continue;
                }
            }
            _ => {}
        }
        out.push(tree.clone());
    }
    out.into_iter().collect()
}

/// Whether the group at `at` is read by a macro: `m!(..)`, `macro_rules! m {..}`, `#[..]`. A
/// group after a `!` that negates it is taken for a macro call too, which only compares more.
fn read_by_macro(seq: &[TokenTree], at: usize) -> bool {
    let before = |back: usize| at.checked_sub(back).map(|i| &seq[i]);
    let named_macro = matches!(before(1), Some(TokenTree::Ident(_)))
        && before(2).is_some_and(|tree| is_punct(tree, '!'))
        && matches!(before(3), Some(TokenTree::Ident(ident)) if !is_keyword(ident));
    before(1).is_some_and(|tree| is_punct(tree, '!') || is_punct(tree, '#')) || named_macro
}

/// Whether the parenthesised group at `at` follows a name it calls or declares (`f(..)`,
/// `fn f<T>(..)`, `S::<T>(..)`, `Fn(..)`), so it is an argument or parameter list.
fn names_a_list(seq: &[TokenTree], at: usize) -> bool {
    let Some(before) = at.checked_sub(1) else {
        return false;
    };
    match &seq[before] {
        TokenTree::Ident(_) => is_name(seq, before),
        tree if is_punct(tree, '>') => closes_generics_of_a_name(seq, before),
        _ => false,
    }
}

/// Whether the identifier at `at` is a name: not a keyword, and not a label or lifetime
/// (`break 'a (x,)` breaks with a tuple).
fn is_name(seq: &[TokenTree], at: usize) -> bool {
    matches!(&seq[at], TokenTree::Ident(ident) if !is_keyword(ident))
        && !at
            .checked_sub(1)
            .is_some_and(|tick| is_punct(&seq[tick], '\''))
}

/// Whether the `>` at `gt` closes generic arguments or parameters that follow a name or `::`.
/// A comparison `a > (b,)` has no matching `<`, and `->` or `=>` closes nothing.
fn closes_generics_of_a_name(seq: &[TokenTree], gt: usize) -> bool {
    let arrow = |i: usize| {
        i.checked_sub(1)
            .is_some_and(|m| is_joint(&seq[m], '-') || is_joint(&seq[m], '='))
    };
    if arrow(gt) {
        return false;
    }
    let mut depth = 0usize;
    for i in (0..=gt).rev() {
        if is_punct(&seq[i], '>') && !arrow(i) {
            depth += 1;
        } else if is_punct(&seq[i], '<') {
            depth -= 1;
            if depth == 0 {
                return i
                    .checked_sub(1)
                    .is_some_and(|before| is_punct(&seq[before], ':') || is_name(seq, before));
            }
        }
    }
    false
}

fn is_keyword(ident: &proc_macro2::Ident) -> bool {
    KEYWORDS.iter().any(|keyword| ident == keyword)
}

#[cfg(test)]
mod tests {
    use quote::ToTokens;

    use super::*;

    /// Whether two Rust files compare equal in the sugar check, printed by `syn` as the check
    /// gets them.
    fn same(a: &str, b: &str) -> bool {
        let tokens = |source: &str| {
            let file = syn::parse_file(source).unwrap().into_token_stream();
            without_trailing_commas(file, MacroTokens::Kept).to_string()
        };
        tokens(a) == tokens(b)
    }

    #[test]
    fn a_trailing_comma_in_a_where_list_is_ignored() {
        assert!(same(
            "fn f<T, U>(t: T) -> u8 where T: Clone, U: Into<Vec<u8>>, { 1 }",
            "fn f<T, U>(t: T) -> u8 where T: Clone, U: Into<Vec<u8>> { 1 }"
        ));
        assert!(same(
            "impl<T> S<T> where T: Fn() -> u8, {}",
            "impl<T> S<T> where T: Fn() -> u8 {}"
        ));
        assert!(same(
            "struct S<T> where T: Copy, { t: T }",
            "struct S<T> where T: Copy { t: T }"
        ));
    }

    #[test]
    fn a_trailing_comma_in_generics_is_ignored() {
        assert!(same(
            "fn f<T: Clone, U,>(x: Vec<T,>) -> Option<Vec<U>,> { None }",
            "fn f<T: Clone, U>(x: Vec<T>) -> Option<Vec<U>> { None }"
        ));
        assert!(same(
            "fn f() { g::<u8, i8,>(); }",
            "fn f() { g::<u8, i8>(); }"
        ));
    }

    #[test]
    fn a_trailing_comma_in_parameters_and_arguments_is_ignored() {
        assert!(same("fn f(x: u8,) {}", "fn f(x: u8) {}"));
        assert!(same("fn f<T>(x: T,) {}", "fn f<T>(x: T) {}"));
        assert!(same("fn f(x: u8, y: u8,) {}", "fn f(x: u8, y: u8) {}"));
        assert!(same(
            "fn f() { g(1,); h::<u8>(2,); x.m(3,); S(4,); }",
            "fn f() { g(1); h::<u8>(2); x.m(3); S(4); }"
        ));
        assert!(same(
            "fn f(g: impl Fn(u8,) -> u8,) {}",
            "fn f(g: impl Fn(u8) -> u8) {}"
        ));
    }

    #[test]
    fn a_trailing_comma_in_a_struct_literal_or_definition_is_ignored() {
        assert!(same(
            "struct S { a: u8, } fn f() { let s = S { a: 1, }; let [x, y,] = [1, 2,]; }",
            "struct S { a: u8 } fn f() { let s = S { a: 1 }; let [x, y] = [1, 2]; }"
        ));
    }

    #[test]
    fn the_comma_of_a_one_element_tuple_is_kept() {
        assert!(!same("fn f() { let t = (1,); }", "fn f() { let t = (1); }"));
        assert!(!same("type T = (u8,);", "type T = (u8);"));
        assert!(!same("fn f(t: (u8,)) {}", "fn f(t: (u8)) {}"));
        assert!(!same("fn f() -> (u8,) { (1,) }", "fn f() -> (u8) { (1) }"));
        assert!(!same("fn f() { let (x,) = t; }", "fn f() { let (x) = t; }"));
        assert!(!same(
            "fn f() { 'a: loop { break 'a (1,); } }",
            "fn f() { 'a: loop { break 'a (1); } }"
        ));
        assert!(!same(
            "fn f() -> bool { a > (b,) }",
            "fn f() -> bool { a > (b) }"
        ));
        assert!(same(
            "fn f() { let t = (1, 2,); }",
            "fn f() { let t = (1, 2); }"
        ));
    }

    #[test]
    fn a_trailing_comma_in_tokens_a_macro_reads_is_kept() {
        assert!(!same("fn f() { m!(a, b,); }", "fn f() { m!(a, b); }"));
        assert!(!same("fn f() { m!(g(a,)); }", "fn f() { m!(g(a)); }"));
        assert!(!same("fn f() { vec![a, b,]; }", "fn f() { vec![a, b]; }"));
        assert!(!same("#[attr(a, b,)] fn f() {}", "#[attr(a, b)] fn f() {}"));
        assert!(!same(
            "macro_rules! m { ($a:expr,) => {}; }",
            "macro_rules! m { ($a:expr) => {}; }"
        ));
    }

    #[test]
    fn a_comma_before_a_block_outside_a_where_list_is_kept() {
        assert!(!same("fn f() { g(S, {a}); }", "fn f() { g(S {a}); }"));
        // A block in generic arguments does not end the `where` list.
        assert!(same(
            "fn f<T>() where T: Tr<{ 1 }>, {}",
            "fn f<T>() where T: Tr<{ 1 }> {}"
        ));
    }

    #[test]
    fn with_macro_tokens_ignored_a_tuple_comma_still_counts() {
        let tokens = |source: &str| {
            let tokens: TokenStream = source.parse().unwrap();
            without_trailing_commas(tokens, MacroTokens::Ignored).to_string()
        };
        assert_eq!(tokens("vec![a, b,]"), tokens("vec![a, b]"));
        assert_eq!(tokens("m!(a, b,)"), tokens("m!(a, b)"));
        assert_ne!(tokens("m!(a,)"), tokens("m!(a)"));
    }
}
