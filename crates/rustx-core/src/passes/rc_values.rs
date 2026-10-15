//! `rc e` expressions (problem 16): `rc e` → `::std::rc::Rc::new(::std::cell::RefCell::new(e))`.
//!
//! A token pass: a name followed by an expression is not valid Rust, so it is rewritten before
//! parsing. It runs right after `prefix_types`, which has already turned every `rc T` in a type
//! position into `Rc<RefCell<T>>`, so an `rc` left here is the value sugar or a plain name.
//!
//! # Detection
//!
//! `rc` is an ordinary identifier, so it is sugar only when the next token cannot follow a name
//! in valid Rust and starts an expression:
//!
//! - an identifier other than `as`, `else`, `for`, `if`, `in` and `where`, which do follow names
//!   (`rc as u8`, `let Some(x) = rc else`, `impl rc for T`, `rc if rc > 0 =>`, `for rc in v`,
//!   `-> rc where`), so `rc x`, `rc vec![..]`, `rc move || ..` and `rc match x { .. }` are sugar;
//! - a literal (`rc 0`, `rc "name"`);
//! - a lone `!` that does not start a macro call (`rc !done`, but not `rc!(..)` or `rc != x`).
//!
//! Everything else keeps `rc` a name: `rc(x)`, `rc[0]`, `rc.f()`, `rc { .. }`, `rc = 1`, `rc?`,
//! and every binary operator, so a value that starts with `&`, `-`, `*` or `|` (`rc & mask`,
//! `rc - 1`, `rc | flags` are valid Rust) needs the plain spelling. After `.`, `::`, `$`, `'` or
//! `#` the name is a field, a path segment, a macro variable, a label or a `quote!` variable, and
//! `r#rc` always means the identifier.
//!
//! An identifier that starts no expression (`rc fn`, `rc dyn Shape`) is an error on `rc`: it is
//! most often a type written where `prefix_types` cannot see a type position.
//!
//! # End of the value
//!
//! Like the operand of `return`, the value runs to the next `,` or `;` of its sequence or to the
//! end of its group, so `rc a + b` wraps `a + b`. Commas inside generic arguments after `::` or a
//! method name (`rc HashMap::<K, V>::new()`, `rc it.collect<HashMap<K, V>>()`) and inside the
//! parameters of a leading closure (`rc move |a, b| a + b`) do not end it.

use proc_macro2::{Spacing, Span, TokenStream, TokenTree};
use quote::quote_spanned;

use crate::tokens::angle::matching_close;
use crate::tokens::map_sequences_reporting;
use crate::tokens::punct::{is_alone, is_punct, is_single_colon};
use crate::tokens::syntax::combine;

/// Keywords that can follow a name in valid Rust, so `rc` before them is the identifier.
const FOLLOWS_A_NAME: &[&str] = &["as", "else", "for", "if", "in", "where"];

/// Keywords that start no expression, so `rc` before them is a misuse rather than a value.
const NOT_A_VALUE: &[&str] = &[
    "dyn", "enum", "extern", "fn", "impl", "let", "mod", "mut", "pub", "ref", "struct", "trait",
    "type", "use",
];

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    map_sequences_reporting(tokens, rewrite_sequence)
}

fn rewrite_sequence(seq: &[TokenTree], errors: &mut Option<syn::Error>) -> Vec<TokenTree> {
    // Detection reads the sequence as written; expansion runs right to left, so an inner `rc` is
    // plain Rust by the time the outer value is cut, and the indices found here stay valid.
    let mut found = Vec::new();
    for i in (0..seq.len()).filter(|&i| is_sugar(seq, i)) {
        match &seq[i + 1] {
            TokenTree::Ident(next) if NOT_A_VALUE.iter().any(|kw| next == kw) => {
                combine(errors, not_a_value(seq[i].span()));
            }
            _ => found.push(i),
        }
    }
    let mut out = seq.to_vec();
    for &i in found.iter().rev() {
        let end = value_end(&out, i + 1);
        let value: TokenStream = out[i + 1..end].iter().cloned().collect();
        let span = out[i].span();
        let wrapped = quote_spanned!(span=> ::std::rc::Rc::new(::std::cell::RefCell::new(#value)));
        out.splice(i..end, wrapped);
    }
    out
}

fn is_sugar(seq: &[TokenTree], i: usize) -> bool {
    let TokenTree::Ident(name) = &seq[i] else {
        return false;
    };
    if name != "rc"
        || i.checked_sub(1)
            .is_some_and(|p| names_something_else(seq, p))
    {
        return false;
    }
    match seq.get(i + 1) {
        Some(TokenTree::Ident(next)) => !FOLLOWS_A_NAME.iter().any(|kw| next == kw),
        Some(TokenTree::Literal(_)) => true,
        Some(TokenTree::Punct(bang)) if bang.as_char() == '!' => {
            bang.spacing() == Spacing::Alone && !matches!(seq.get(i + 2), Some(TokenTree::Group(_)))
        }
        _ => false,
    }
}

/// Whether the token at `p` makes the name after it something other than a value: `.rc`,
/// `a::rc`, `$rc`, `'rc` or `#rc`.
fn names_something_else(seq: &[TokenTree], p: usize) -> bool {
    match &seq[p] {
        TokenTree::Punct(punct) => match punct.as_char() {
            '.' | '$' | '\'' | '#' => true,
            ':' => !is_single_colon(seq, p),
            _ => false,
        },
        _ => false,
    }
}

/// Index just past the value that starts at `start`.
fn value_end(seq: &[TokenTree], start: usize) -> usize {
    let mut at = start;
    while at < seq.len() && !is_punct(&seq[at], ',') && !is_punct(&seq[at], ';') {
        at = bracket_close(seq, start, at).unwrap_or(at) + 1;
    }
    at
}

/// The closing `>` or `|` of generic arguments or closure parameters opened at `at`, whose
/// commas belong to the value.
fn bracket_close(seq: &[TokenTree], start: usize, at: usize) -> Option<usize> {
    let before = at.checked_sub(1).filter(|&p| p >= start);
    if is_punct(&seq[at], '<') {
        let p = before?;
        let generics = match &seq[p] {
            TokenTree::Punct(_) => !is_single_colon(seq, p) && is_punct(&seq[p], ':'),
            TokenTree::Ident(_) => p > start && is_punct(&seq[p - 1], '.'),
            _ => false,
        };
        return generics.then(|| matching_close(seq, at)).flatten();
    }
    let opens_closure = is_alone(&seq[at], '|')
        && before.is_none_or(|p| matches!(&seq[p], TokenTree::Ident(kw) if kw == "move"));
    if opens_closure {
        return (at + 1..seq.len()).find(|&k| is_punct(&seq[k], '|'));
    }
    None
}

fn not_a_value(span: Span) -> syn::Error {
    syn::Error::new(
        span,
        "expected an expression after `rc`, as in `rc value`; a type here needs the plain \
         `Rc<RefCell<T>>` spelling",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    fn expands_to(sugar: TokenStream, plain: &TokenStream) {
        assert_eq!(rewrite(sugar).unwrap().to_string(), plain.to_string());
    }

    fn unchanged(valid: &TokenStream) {
        assert_eq!(
            rewrite(valid.clone()).unwrap().to_string(),
            valid.to_string()
        );
    }

    #[test]
    fn let_value_becomes_rc_of_refcell() {
        expands_to(
            quote! { let a = rc 5; },
            &quote! { let a = ::std::rc::Rc::new(::std::cell::RefCell::new(5)); },
        );
    }

    #[test]
    fn each_argument_ends_at_its_comma() {
        expands_to(
            quote! { f(rc a, rc "b", c) },
            &quote! { f(::std::rc::Rc::new(::std::cell::RefCell::new(a)),
            ::std::rc::Rc::new(::std::cell::RefCell::new("b")), c) },
        );
    }

    #[test]
    fn value_runs_to_the_end_like_a_return_operand() {
        expands_to(
            quote! { let a = rc x.size + 1; },
            &quote! { let a = ::std::rc::Rc::new(::std::cell::RefCell::new(x.size + 1)); },
        );
    }

    #[test]
    fn struct_literal_at_the_tail_is_one_value() {
        expands_to(
            quote! { { rc Node { name, size: 0 } } },
            &quote! { { ::std::rc::Rc::new(::std::cell::RefCell::new(Node { name, size: 0 })) } },
        );
    }

    #[test]
    fn macro_and_keyword_values_are_sugar() {
        expands_to(
            quote! { (rc vec![1], rc move || 1, rc match x { _ => 1 }, rc !done, rc true) },
            &quote! { (
                ::std::rc::Rc::new(::std::cell::RefCell::new(vec![1])),
                ::std::rc::Rc::new(::std::cell::RefCell::new(move || 1)),
                ::std::rc::Rc::new(::std::cell::RefCell::new(match x { _ => 1 })),
                ::std::rc::Rc::new(::std::cell::RefCell::new(!done)),
                ::std::rc::Rc::new(::std::cell::RefCell::new(true))
            ) },
        );
    }

    #[test]
    fn nested_values_expand_inside_out() {
        expands_to(
            quote! { let t = rc vec![rc a, rc b]; },
            &quote! { let t = ::std::rc::Rc::new(::std::cell::RefCell::new(vec![
                ::std::rc::Rc::new(::std::cell::RefCell::new(a)),
                ::std::rc::Rc::new(::std::cell::RefCell::new(b))
            ])); },
        );
        expands_to(
            quote! { rc rc x },
            &quote! { ::std::rc::Rc::new(::std::cell::RefCell::new(
            ::std::rc::Rc::new(::std::cell::RefCell::new(x)))) },
        );
    }

    #[test]
    fn commas_in_generics_and_closure_parameters_stay_in_the_value() {
        expands_to(
            quote! { (rc HashMap::<u8, u8>::new(), rc it.collect<HashMap<K, V>>(),
            rc move |a, b| a + b, rc |a, b| a) },
            &quote! { (
                ::std::rc::Rc::new(::std::cell::RefCell::new(HashMap::<u8, u8>::new())),
                ::std::rc::Rc::new(::std::cell::RefCell::new(it.collect<HashMap<K, V>>())),
                ::std::rc::Rc::new(::std::cell::RefCell::new(move |a, b| a + b)),
                rc |a, b| a
            ) },
        );
    }

    #[test]
    fn comparisons_still_end_at_a_comma() {
        expands_to(
            quote! { (rc a < b, c > d) },
            &quote! { (::std::rc::Rc::new(::std::cell::RefCell::new(a < b)), c > d) },
        );
    }

    #[test]
    fn rc_used_as_a_name_stays() {
        unchanged(&quote! {
            use std::rc::{self, Rc};
            impl rc for X {}
            fn rc(rc: u8) -> rc where rc: Copy {
                let rc = rc(x) + rc[0] + rc.f() + rc.rc;
                rc = 1;
                f(rc, x);
                if rc { } else { S { rc } }
                let q = rc < <Vec<u8>>::LEN;
                let n = rc as u8 + rc? - rc & mask | flags * rc;
                let Some(v) = rc else { return };
                for rc in v {}
                match v { rc if rc > 0 => rc!(rc), _ => rc != a::rc }
                rc::x();
                break 'rc x;
            }
        });
    }

    #[test]
    fn macro_variables_quote_variables_and_raw_names_stay() {
        // A string, because `quote!` here would read `#rc` as its own interpolation.
        unchanged(
            &"macro_rules! m { ($rc:expr) => { $rc + 1 }; (rc $x:expr) => { rc }; }
              quote!(#rc x);
              let a = r#rc x;"
                .parse()
                .unwrap(),
        );
    }

    #[test]
    fn a_keyword_that_starts_no_expression_is_an_error() {
        let err = rewrite(quote! { fn f() -> (rc dyn Shape, rc mut x) {} }).unwrap_err();
        let messages: Vec<String> = err.into_iter().map(|e| e.to_string()).collect();
        assert_eq!(messages.len(), 2);
        assert!(
            messages[0].starts_with("expected an expression after `rc`"),
            "{messages:?}"
        );
    }
}
