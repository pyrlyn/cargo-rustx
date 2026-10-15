//! Method generics without turbofish: `.m<T>()` → `.m::<T>()` (problem 06).
//!
//! A token pass: without `::` the call parses as comparisons, so it is rewritten before parsing.
//!
//! The pattern is `.` method `<` args `>` `(..)`: the angle brackets must balance (each `>` of
//! a `>>` closes one level, and the `>` of `->` closes none), the closing `>` must be directly
//! followed by a parenthesised group, and the args must parse as generic arguments with `syn`.
//! Anything else, including an existing `::<`, is left for the parser.
//!
//! Known std collections written bare as a whole argument get their parameters as `_`, because
//! their arity is fixed and `_` is what the user would have typed: `collect<Vec>()` →
//! `collect::<Vec<_>>()`, `collect<BTreeMap>()` → `collect::<BTreeMap<_, _>>()`. `String` takes
//! no parameters; `Option`, `Result`, `Box`, `Rc` and `Arc` are left alone because the problem
//! file does not use them and `Result` is often a one-parameter alias.
//!
//! Ambiguity: `x.y < z > (w)` alone is rejected by rustc (comparison operators cannot be
//! chained), so reading it as a call loses no valid program. In a comma-separated list,
//! `(a.b < c, d > (e))` is valid Rust (two comparisons) and is still read as the call
//! `a.b::<c, d>(e)`. This is the one place the sugar changes the meaning of valid Rust; the
//! comparisons need parentheses inside `sugar!`, for example `((a.b < c), d > (e))`.

use proc_macro2::{Delimiter, Punct, Spacing, Span, TokenStream, TokenTree};
use quote::ToTokens;
use syn::punctuated::Punctuated;
use syn::{AngleBracketedGenericArguments, GenericArgument, PathArguments, Type, TypeInfer};

use crate::tokens::map_sequences;

/// Std collections whose arity is fixed, with the number of `_` they need when written bare.
const COLLECTIONS: &[(&str, usize)] = &[
    ("Vec", 1),
    ("VecDeque", 1),
    ("LinkedList", 1),
    ("BinaryHeap", 1),
    ("HashSet", 1),
    ("BTreeSet", 1),
    ("HashMap", 2),
    ("BTreeMap", 2),
];

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    map_sequences(tokens, &mut |seq| Ok(rewrite_sequence(&seq)))
}

fn rewrite_sequence(seq: &[TokenTree]) -> Vec<TokenTree> {
    let mut out = Vec::with_capacity(seq.len());
    let mut i = 0;
    while i < seq.len() {
        if let Some((close, args)) = method_generics(seq, i) {
            // `.` and the method name stay; `::` takes the span of `<` so errors land on it.
            out.extend_from_slice(&seq[i..i + 2]);
            out.extend(args.into_token_stream());
            i = close + 1;
        } else {
            out.push(seq[i].clone());
            i += 1;
        }
    }
    out
}

/// Matches `.` method `<` .. `>` `(` at `dot` and returns the index of the closing `>` together
/// with the arguments as a turbofish, already filled.
fn method_generics(
    seq: &[TokenTree],
    dot: usize,
) -> Option<(usize, AngleBracketedGenericArguments)> {
    if !is_punct(seq.get(dot)?, '.') || !matches!(seq.get(dot + 1)?, TokenTree::Ident(_)) {
        return None;
    }
    // The second `.` of `..` / `..=` starts no method call: `0..n<T>(x)` is a range.
    if dot > 0 && is_joint_punct(&seq[dot - 1], '.') {
        return None;
    }
    let open = dot + 2;
    let TokenTree::Punct(lt) = seq.get(open)? else {
        return None;
    };
    if lt.as_char() != '<' {
        return None;
    }
    let close = matching_close(seq, open)?;
    match seq.get(close + 1)? {
        TokenTree::Group(group) if group.delimiter() == Delimiter::Parenthesis => {}
        _ => return None,
    }
    let angle: TokenStream = seq[open..=close].iter().cloned().collect();
    let mut args: AngleBracketedGenericArguments = syn::parse2(angle).ok()?;
    if args.args.is_empty() {
        return None;
    }
    args.colon2_token = Some(syn::Token![::](lt.span()));
    for arg in &mut args.args {
        fill_collection(arg);
    }
    Some((close, args))
}

/// Index of the `>` that closes the `<` at `open`, or `None` if the sequence ends first.
fn matching_close(seq: &[TokenTree], open: usize) -> Option<usize> {
    let mut depth = 0usize;
    for (index, tree) in seq.iter().enumerate().skip(open) {
        let TokenTree::Punct(punct) = tree else {
            continue;
        };
        match punct.as_char() {
            '<' => depth += 1,
            // The `>` of `->` (as in `Box<dyn Fn() -> u8>`) closes nothing.
            '>' if !is_joint_punct(&seq[index - 1], '-') => {
                depth -= 1;
                if depth == 0 {
                    return Some(index);
                }
            }
            // A statement end means this `<` was a comparison; stop instead of scanning on.
            ';' => return None,
            _ => {}
        }
    }
    None
}

/// Turns a bare known collection such as `Vec` into `Vec<_>`, spans taken from its name.
fn fill_collection(arg: &mut GenericArgument) {
    let GenericArgument::Type(Type::Path(ty)) = arg else {
        return;
    };
    if ty.qself.is_some() || ty.path.leading_colon.is_some() || ty.path.segments.len() != 1 {
        return;
    }
    let Some(segment) = ty.path.segments.first_mut() else {
        return;
    };
    if !matches!(segment.arguments, PathArguments::None) {
        return;
    }
    let Some(&(_, arity)) = COLLECTIONS.iter().find(|(name, _)| segment.ident == name) else {
        return;
    };
    let span = segment.ident.span();
    segment.arguments = PathArguments::AngleBracketed(AngleBracketedGenericArguments {
        colon2_token: None,
        lt_token: syn::Token![<](span),
        args: infer_args(arity, span),
        gt_token: syn::Token![>](span),
    });
}

fn infer_args(arity: usize, span: Span) -> Punctuated<GenericArgument, syn::Token![,]> {
    let mut args = Punctuated::new();
    for index in 0..arity {
        if index > 0 {
            args.push_punct(syn::Token![,](span));
        }
        args.push_value(GenericArgument::Type(Type::Infer(TypeInfer {
            attrs: Vec::new(),
            underscore_token: syn::Token![_](span),
        })));
    }
    args
}

fn is_punct(tree: &TokenTree, ch: char) -> bool {
    matches!(tree, TokenTree::Punct(punct) if punct.as_char() == ch)
}

fn is_joint_punct(tree: &TokenTree, ch: char) -> bool {
    matches!(tree, TokenTree::Punct(punct) if is_joint(punct, ch))
}

fn is_joint(punct: &Punct, ch: char) -> bool {
    punct.as_char() == ch && punct.spacing() == Spacing::Joint
}

#[cfg(test)]
mod tests {
    use super::*;

    // Printed token spacing depends on `Spacing`, which says nothing about meaning; compare
    // without whitespace so `::<` from syn equals `:: <` from the lexer.
    fn compact(tokens: &TokenStream) -> String {
        tokens.to_string().split_whitespace().collect()
    }

    fn expand(source: &str) -> String {
        compact(&rewrite(source.parse().unwrap()).unwrap())
    }

    fn same(source: &str) -> String {
        compact(&source.parse().unwrap())
    }

    #[test]
    fn method_generics_become_a_turbofish() {
        assert_eq!(expand("s.parse<u32>()?"), same("s.parse::<u32>()?"));
    }

    #[test]
    fn several_and_nested_arguments_are_kept() {
        assert_eq!(
            expand("it.collect<HashMap<String, Vec<u8>>>()"),
            same("it.collect::<HashMap<String, Vec<u8>>>()")
        );
        assert_eq!(
            expand("it.unzip<Vec<u8>, Vec<u16>>()"),
            same("it.unzip::<Vec<u8>, Vec<u16>>()")
        );
    }

    #[test]
    fn bare_collections_get_inferred_parameters() {
        assert_eq!(expand("it.collect<Vec>()"), same("it.collect::<Vec<_>>()"));
        assert_eq!(
            expand("it.collect<HashSet>()"),
            same("it.collect::<HashSet<_>>()")
        );
        assert_eq!(
            expand("it.collect<BTreeMap>()"),
            same("it.collect::<BTreeMap<_, _>>()")
        );
        assert_eq!(
            expand("it.unzip<Vec, VecDeque>()"),
            same("it.unzip::<Vec<_>, VecDeque<_>>()")
        );
    }

    #[test]
    fn non_collections_and_paths_are_not_filled() {
        assert_eq!(
            expand("it.collect<String>()"),
            same("it.collect::<String>()")
        );
        assert_eq!(
            expand("it.collect<Result>()"),
            same("it.collect::<Result>()")
        );
        assert_eq!(
            expand("it.collect<my::Vec>()"),
            same("it.collect::<my::Vec>()")
        );
    }

    #[test]
    fn chained_calls_are_each_rewritten() {
        assert_eq!(
            expand("a.map<u8>(f).collect<Vec>().len()"),
            same("a.map::<u8>(f).collect::<Vec<_>>().len()")
        );
    }

    #[test]
    fn calls_inside_groups_are_rewritten() {
        assert_eq!(
            expand("fn f() { g(x.parse<u8>()); }"),
            same("fn f() { g(x.parse::<u8>()); }")
        );
    }

    #[test]
    fn arrow_inside_arguments_does_not_close_them() {
        assert_eq!(
            expand("x.boxed<Box<dyn Fn() -> u8>>()"),
            same("x.boxed::<Box<dyn Fn() -> u8>>()")
        );
    }

    #[test]
    fn existing_turbofish_is_unchanged() {
        let src = "it.collect::<Vec<_>>()";
        assert_eq!(expand(src), same(src));
    }

    #[test]
    fn comparisons_are_unchanged() {
        for src in [
            "a.b < c",
            "a.b <= c",
            "a.len() < b.len()",
            "a.b < c && d > (e)",
            "a.b < c >> (d)",
            "a.b << c >> (d)",
            "if a.b < c { x } else { y > (z) }",
            "a.b < c; d > (e)",
            "t.0 < c > (d)",
            "0..n < c > (d)",
        ] {
            assert_eq!(expand(src), same(src), "{src}");
        }
    }

    #[test]
    fn generics_not_followed_by_a_call_are_unchanged() {
        assert_eq!(expand("a.b < c > d"), same("a.b < c > d"));
        assert_eq!(expand("a.b < c > [d]"), same("a.b < c > [d]"));
    }

    #[test]
    fn empty_angle_brackets_are_left_to_the_parser() {
        assert_eq!(expand("a.b<>()"), same("a.b<>()"));
    }

    #[test]
    fn chained_comparison_with_call_reads_as_sugar() {
        // rustc rejects `x.y < z > (w)`, so no valid program changes meaning.
        assert_eq!(expand("x.y < z > (w)"), same("x.y::<z>(w)"));
    }

    #[test]
    fn comma_separated_comparisons_read_as_sugar() {
        // The documented ambiguity: valid Rust, but the sugar reading wins.
        assert_eq!(expand("(a.b < c, d > (e))"), same("(a.b::<c, d>(e))"));
        let parenthesised = "((a.b < c), d > (e))";
        assert_eq!(expand(parenthesised), same(parenthesised));
    }
}
