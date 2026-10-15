//! Capture lists `move[a.clone(), b.clone()] || body` (problem 11).
//!
//! A token pass: `move[` is not Rust, so the list is taken off before `syn` parses the file. The
//! closure becomes `{ let a = a.clone(); let b = b.clone(); move || body }`, the form users
//! write by hand today, so the names they already use stay.
//!
//! - Names: each binding takes the name of the variable its capture starts with, through a chain
//!   of method calls: `a.clone()` and `a.lock().unwrap().clone()` bind `a`, a bare `a` binds `a`.
//!   Anything else (`Arc::clone(&a)`, `self.cfg.clone()`, `a.b.clone()`, `&a`, a literal) is
//!   rejected: the problem file shows plain variables only, and naming a binding after a field
//!   or inventing a name would hide which value the body sees. A name captured twice is
//!   rejected, because the second binding would silently shadow the first.
//! - Closures: `move[..] |args| body`, and `async move[..] |args| body` for an async closure,
//!   with the list where Rust puts `move`. An async block or anything else after the list is
//!   rejected; the problem file shows closures only. An empty list is rejected too, since
//!   `move ||` already says that.
//! - Extent: the closure is read with `syn`, so it ends exactly where Rust ends it: at the `,` or
//!   `;` after its body, or after the block when a return type is written. That needs tokens
//!   without sugar, so this pass runs after every other token pass, and each sequence is
//!   rewritten from the right, so a capture list in a closure body is expanded before the
//!   closure around it is read.
//! - Upstream: the list keeps the shape of `move(expr)` from RFC 3968 (a value computed when the
//!   closure is created), so switching to the native form changes the expansion, not user code.
//!
//! Like the other token passes, it also rewrites inside macro-call arguments: a user macro that
//! takes `move[..]` tokens itself receives the expansion.

use std::ops::Range;

use proc_macro2::{Delimiter, Group, Ident, TokenStream, TokenTree};
use quote::quote_spanned;
use syn::parse::{ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Expr, ExprClosure, Token};

use crate::tokens::map_sequences_reporting;
use crate::tokens::punct::is_punct;
use crate::tokens::syntax::combine;

const EMPTY: &str = "a capture list needs at least one capture; without captures write `move ||`";
const NO_NAME: &str = "a capture is named after the variable it starts with (`a.clone()` binds \
                       `a`); bind this value with `let` before the closure and capture that";
const NOT_A_CLOSURE: &str = "`move[..]` must be followed by a closure: `move[..] |args| body`, \
                             or `async move[..] |args| body` for an async closure";

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    map_sequences_reporting(tokens, rewrite_sequence)
}

fn rewrite_sequence(seq: &[TokenTree], errors: &mut Option<syn::Error>) -> Vec<TokenTree> {
    let mut seq = seq.to_vec();
    // From the right: what follows a capture list must already be plain Rust for `syn`.
    for at in (0..seq.len()).rev() {
        let (TokenTree::Ident(keyword), Some(TokenTree::Group(list))) = (&seq[at], seq.get(at + 1))
        else {
            continue;
        };
        if keyword != "move" || list.delimiter() != Delimiter::Bracket {
            continue;
        }
        match expand(&seq, at, keyword, list) {
            Ok((range, block)) => drop(seq.splice(range, [block])),
            Err(err) => combine(errors, err),
        }
    }
    seq
}

/// The tokens the capture list at `at` and its closure span, and the block replacing them.
fn expand(
    seq: &[TokenTree],
    at: usize,
    keyword: &Ident,
    list: &Group,
) -> syn::Result<(Range<usize>, TokenTree)> {
    let start = match at.checked_sub(1).map(|before| &seq[before]) {
        Some(TokenTree::Ident(asyncness)) if asyncness == "async" => at - 1,
        _ => at,
    };
    let bindings = bindings(keyword, list)?;
    let closure = at + 2;
    if !seq.get(closure).is_some_and(|tree| is_punct(tree, '|')) {
        return Err(syn::Error::new(keyword.span(), NOT_A_CLOSURE));
    }
    let end = closure + closure_len(&seq[closure..])?;
    let mut body = bindings;
    body.extend(seq[start..at].iter().cloned());
    body.extend([TokenTree::Ident(keyword.clone())]);
    body.extend(seq[closure..end].iter().cloned());
    let mut block = Group::new(Delimiter::Brace, body);
    block.set_span(keyword.span());
    Ok((start..end, TokenTree::Group(block)))
}

/// `let name = capture;` for every capture in `list`, or every reason one has no name.
fn bindings(keyword: &Ident, list: &Group) -> syn::Result<TokenStream> {
    let captures = Punctuated::<Expr, Token![,]>::parse_terminated.parse2(list.stream())?;
    if captures.is_empty() {
        return Err(syn::Error::new(list.span(), EMPTY));
    }
    let mut errors = None;
    let mut names: Vec<&Ident> = Vec::with_capacity(captures.len());
    let mut out = TokenStream::new();
    for capture in &captures {
        match binding_name(capture) {
            None => combine(&mut errors, syn::Error::new(capture.span(), NO_NAME)),
            Some(name) if names.contains(&name) => {
                let message = format!("`{name}` is already captured in this list");
                combine(&mut errors, syn::Error::new(name.span(), message));
            }
            Some(name) => {
                // The name keeps the user's span, so the closure body resolves to this binding.
                out.extend(quote_spanned!(keyword.span()=> let #name = #capture;));
                names.push(name);
            }
        }
    }
    errors.map_or(Ok(out), Err)
}

/// The variable a capture starts with, followed only through method-call receivers.
fn binding_name(capture: &Expr) -> Option<&Ident> {
    match capture {
        Expr::MethodCall(call) => binding_name(&call.receiver),
        Expr::Path(path) if path.qself.is_none() => path
            .path
            .get_ident()
            .filter(|name| !matches!(name.to_string().as_str(), "self" | "Self")),
        _ => None,
    }
}

/// How many of `tokens`, which start at the closure's first `|`, the closure takes.
fn closure_len(tokens: &[TokenTree]) -> syn::Result<usize> {
    let rest = |input: ParseStream| {
        input.parse::<ExprClosure>()?;
        Ok(input.parse::<TokenStream>()?.into_iter().count())
    };
    let rest = rest.parse2(tokens.iter().cloned().collect())?;
    Ok(tokens.len() - rest)
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    fn expand(input: TokenStream) -> syn::Result<String> {
        rewrite(input).map(|out| out.to_string())
    }

    fn assert_expands(input: TokenStream, want: &TokenStream) {
        assert_eq!(expand(input).unwrap(), want.to_string());
    }

    fn assert_rejected(input: TokenStream, message: &str) {
        let shown = input.to_string();
        let err = expand(input).unwrap_err();
        assert!(err.to_string().contains(message), "{shown}: {err}");
    }

    #[test]
    fn a_capture_list_becomes_let_bindings_before_a_move_closure() {
        assert_expands(
            quote! { spawn(move[a.clone(), b.clone()] || work(a, b)); },
            &quote! { spawn({ let a = a.clone(); let b = b.clone(); move || work(a, b) }); },
        );
    }

    #[test]
    fn a_capture_binds_the_variable_its_method_chain_starts_with() {
        assert_expands(
            quote! { move[rx.lock().unwrap().clone(), n, v.iter::<u8>(),] || () },
            &quote! {{
                let rx = rx.lock().unwrap().clone(); let n = n; let v = v.iter::<u8>();
                move || ()
            }},
        );
    }

    #[test]
    fn the_closure_ends_where_rust_ends_it() {
        assert_expands(
            quote! {
                f(move[a.clone()] |x, y: HashMap<u8, u8>| x < a, c > d);
                let g = move[a] || a + 1;
                let h = move[a] || -> Vec<u8> { a } ;
            },
            &quote! {
                f({ let a = a.clone(); move |x, y: HashMap<u8, u8>| x < a }, c > d);
                let g = { let a = a; move || a + 1 };
                let h = { let a = a; move || -> Vec<u8> { a } } ;
            },
        );
    }

    #[test]
    fn an_async_closure_keeps_async_in_front_of_move() {
        assert_expands(
            quote! { run(async move[a.clone()] |x| a.get(x).await) },
            &quote! { run({ let a = a.clone(); async move |x| a.get(x).await }) },
        );
    }

    #[test]
    fn capture_lists_in_closure_bodies_and_nested_groups_are_expanded() {
        assert_expands(
            quote! { move[a.clone()] || move[b.clone()] || a + b; f(|| { g(move[c] || c) }) },
            &quote! {
                { let a = a.clone(); move || { let b = b.clone(); move || a + b } };
                f(|| { g({ let c = c; move || c }) })
            },
        );
    }

    #[test]
    fn valid_rust_passes_through_unchanged() {
        let input = quote! {
            let f = move || a;
            let g = async move { a };
            let h = async move |x| x;
            let r#move = [1];
            r#move[0];
            v[moved];
            m!(move);
        };
        assert_eq!(expand(input.clone()).unwrap(), input.to_string());
    }

    #[test]
    fn an_empty_capture_list_is_rejected() {
        assert_rejected(quote! { move[] || 1 }, "at least one capture");
    }

    #[test]
    fn a_capture_without_a_variable_to_name_it_is_rejected() {
        for capture in [
            quote! { Arc::clone(&a) },
            quote! { self.cfg.clone() },
            quote! { a.b.clone() },
            quote! { &a },
            quote! { 1 },
            quote! { make() },
            quote! { self },
            quote! { a::b },
            quote! { (a).clone() },
        ] {
            assert_rejected(quote! { move[#capture] || () }, "named after the variable");
        }
    }

    #[test]
    fn a_capture_list_must_be_followed_by_a_closure() {
        for input in [
            quote! { move[a.clone()] async || a },
            quote! { async move[a.clone()] { a } },
            quote! { move[a] x },
            quote! { f(move[a]) },
        ] {
            assert_rejected(input, "must be followed by a closure");
        }
    }

    #[test]
    fn a_variable_captured_twice_is_rejected() {
        assert_rejected(
            quote! { move[a.clone(), a.lock()] || a },
            "`a` is already captured",
        );
    }

    #[test]
    fn every_misuse_in_one_input_is_reported() {
        let err = expand(quote! { move[] || 1; f(move[1, 2] || 3); move[a] }).unwrap_err();
        assert_eq!(err.into_iter().count(), 4);
    }
}
