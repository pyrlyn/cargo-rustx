//! `throw e` in a fn returning `T throws` (problem 10).
//!
//! `throw e` returns `e` as the fn's error:
//!
//! ```text
//! if workers == 0 { throw "workers must be > 0"; }
//! if workers == 0 {
//!     return ::core::result::Result::Err(::rustx::anyhow::anyhow!("workers must be > 0"));
//! }
//! ```
//!
//! With `throws E` the value goes through `::core::convert::Into::into(e)`. With plain `throws`
//! it goes through `anyhow!(e)` instead, because `anyhow::Error` has no `From<&str>`: `anyhow!`
//! keeps an error value as its source and turns any other `Display` value into a message. A
//! string literal is a format string there, so `throw "bad {name}"` interpolates `name`.
//! Generated tokens take the span of `throw`.
//!
//! A token pass, because `throw e` does not parse. It runs right after `throws::rewrite`, whose
//! body marker says which error type the enclosing fn has; `throws::lower` takes it off later.
//!
//! # Detection
//!
//! `throw` is sugar where an expression starts (a statement start as `var` defines it, or after
//! `=>`, `=` or `|`) and an identifier or a literal follows. An identifier `throw` is never
//! followed by either there in valid Rust, except by `as` (a cast), `if` (a match guard) and
//! `else` (`let .. = throw else`), which are left alone, as are `r#throw`, `throw(e)`,
//! `throw.f`, `throw::m::E` and `throw & e`. The thrown value runs like the operand of `return`:
//! to the next `;` or `,` outside a turbofish, or to the end of its group.
//!
//! # Scope
//!
//! The walk is top-down, because a group's meaning depends on what encloses it, while
//! [`map_sequences`](crate::tokens::map_sequences) rewrites the innermost groups first. A marked
//! body sets the error type. The body of an unmarked fn, a closure, an `async` block and a
//! `const` block clear it, because each has its own return type; `throw` there, or outside any
//! fn, is an error.
//!
//! A closure starts at a `|` that cannot be a binary operator or a pattern alternative: one with
//! nothing, a punctuation, `move`, `async`, `return` or a capture list before it. Its body runs
//! to the next `;` or `,`, or to the end of its group.
//!
//! # Limitations
//!
//! - A value that starts with punctuation or a group (`throw &e`, `throw (e)`) is not sugar,
//!   since the same tokens are valid Rust; start it with a name or a literal.
//! - Like `var`, a macro whose input has `throw <name>` gets the rewrite, or the error.
//! - A match arm pattern with a leading `|` (`| A | B => ..`) is read as a closure.

use std::mem;

use proc_macro2::{Delimiter, Group, Span, TokenStream, TokenTree};
use quote::quote_spanned;

use super::throws::{ANYHOW, MARKER};
use crate::tokens::angle::angle_depth;
use crate::tokens::position::starts_statement;
use crate::tokens::punct::{is_alone, is_joint, is_punct};
use crate::tokens::syntax::combine;

const OUTSIDE: &str = "`throw` is only allowed in a fn returning `T throws`";

/// The error type of the enclosing `throws` fn, which decides how a thrown value converts.
#[derive(Clone, Copy)]
enum Form {
    /// Plain `throws`: `anyhow::Error`.
    Anyhow,
    /// `throws E`.
    Into,
}

/// Where the walk is relative to a closure in the current sequence.
#[derive(Clone, Copy, PartialEq)]
enum Closure {
    Outside,
    Parameters,
    Body,
}

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    let seq: Vec<TokenTree> = tokens.into_iter().collect();
    let mut errors = None;
    let out = rewrite_sequence(&seq, None, &mut errors);
    errors.map_or_else(|| Ok(out.into_iter().collect()), Err)
}

fn rewrite_sequence(
    seq: &[TokenTree],
    form: Option<Form>,
    errors: &mut Option<syn::Error>,
) -> Vec<TokenTree> {
    let mut out = Vec::with_capacity(seq.len());
    let mut closure = Closure::Outside;
    let mut fn_body_next = false;
    let mut i = 0;
    while i < seq.len() {
        let here = form.filter(|_| closure == Closure::Outside);
        match &seq[i] {
            TokenTree::Group(group) => {
                let inner = if group.delimiter() == Delimiter::Brace {
                    let fn_body = mem::take(&mut fn_body_next);
                    match marked_form(group) {
                        Some(marked) => Some(marked),
                        None if fn_body || opens_own_block(seq, i) => None,
                        None => here,
                    }
                } else {
                    here
                };
                let inside: Vec<TokenTree> = group.stream().into_iter().collect();
                let stream = rewrite_sequence(&inside, inner, errors)
                    .into_iter()
                    .collect();
                let mut rebuilt = Group::new(group.delimiter(), stream);
                rebuilt.set_span(group.span());
                out.push(TokenTree::Group(rebuilt));
            }
            TokenTree::Ident(word) if word == "throw" && is_sugar(seq, i) => {
                if let Some(form) = here {
                    let end = operand_end(seq, i + 1);
                    let operand = rewrite_sequence(&seq[i + 1..end], Some(form), errors);
                    out.extend(thrown(word.span(), form, &operand));
                    i = end;
                    continue;
                }
                combine(errors, syn::Error::new(word.span(), OUTSIDE));
                out.push(seq[i].clone());
            }
            tree => {
                if let TokenTree::Ident(word) = tree
                    && word == "fn"
                {
                    // `fn name` starts an item; `fn(..)` is a fn-pointer type with no body.
                    fn_body_next |= matches!(seq.get(i + 1), Some(TokenTree::Ident(_)));
                }
                fn_body_next &= !is_punct(tree, ';');
                closure = next_closure(seq, i, closure);
                out.push(tree.clone());
            }
        }
        i += 1;
    }
    out
}

/// The form a `throws` body's marker records, if `body` starts with one.
fn marked_form(body: &Group) -> Option<Form> {
    let mut trees = body.stream().into_iter();
    let (Some(hash), Some(bang), Some(TokenTree::Group(attr))) =
        (trees.next(), trees.next(), trees.next())
    else {
        return None;
    };
    if !(is_punct(&hash, '#') && is_punct(&bang, '!') && attr.delimiter() == Delimiter::Bracket) {
        return None;
    }
    let mut inside = attr.stream().into_iter();
    match (inside.next(), inside.next()) {
        (Some(TokenTree::Ident(name)), None) if name == MARKER => Some(Form::Into),
        (Some(TokenTree::Ident(name)), Some(TokenTree::Group(arg)))
            if name == MARKER && arg.stream().to_string() == ANYHOW =>
        {
            Some(Form::Anyhow)
        }
        _ => None,
    }
}

/// Whether the `{ .. }` at `at` is an `async` or `const` block, which has its own return type.
fn opens_own_block(seq: &[TokenTree], at: usize) -> bool {
    let word = |back: usize| match at.checked_sub(back).map(|before| &seq[before]) {
        Some(TokenTree::Ident(word)) => Some(word.to_string()),
        _ => None,
    };
    match word(1).as_deref() {
        Some("async" | "const") => true,
        Some("move") => word(2).as_deref() == Some("async"),
        _ => false,
    }
}

/// Whether the `throw` at `i` is sugar, as the module header defines it.
fn is_sugar(seq: &[TokenTree], i: usize) -> bool {
    let value_follows = match seq.get(i + 1) {
        Some(TokenTree::Literal(_)) => true,
        Some(TokenTree::Ident(next)) => !["as", "if", "else"].iter().any(|word| next == word),
        _ => false,
    };
    let expression_starts = starts_statement(seq, i)
        || i.checked_sub(1).is_some_and(|before| {
            let arrow = is_punct(&seq[before], '>')
                && before
                    .checked_sub(1)
                    .is_some_and(|eq| is_joint(&seq[eq], '='));
            arrow || is_alone(&seq[before], '=') || is_punct(&seq[before], '|')
        });
    value_follows && expression_starts
}

/// The index right after the thrown value that starts at `start`.
fn operand_end(seq: &[TokenTree], start: usize) -> usize {
    let mut turbofish = 0;
    for at in start..seq.len() {
        let opens = is_punct(&seq[at], '<')
            && at >= start + 2
            && is_punct(&seq[at - 1], ':')
            && is_joint(&seq[at - 2], ':');
        if turbofish > 0 || opens {
            turbofish = angle_depth(seq, at, turbofish);
        } else if is_punct(&seq[at], ';') || is_punct(&seq[at], ',') {
            return at;
        }
    }
    seq.len()
}

/// The closure state after the token at `i`, which is not a group.
fn next_closure(seq: &[TokenTree], i: usize, state: Closure) -> Closure {
    let tree = &seq[i];
    if !is_punct(tree, '|') {
        let ends = is_punct(tree, ';') || is_punct(tree, ',');
        return if state == Closure::Body && ends {
            Closure::Outside
        } else {
            state
        };
    }
    // The second `|` of `||` was settled together with the first.
    if i > 0 && is_joint(&seq[i - 1], '|') {
        return state;
    }
    match state {
        Closure::Parameters => Closure::Body,
        _ if opens_closure(seq, i) => {
            let no_parameters =
                is_joint(tree, '|') && seq.get(i + 1).is_some_and(|next| is_punct(next, '|'));
            if no_parameters {
                Closure::Body
            } else {
                Closure::Parameters
            }
        }
        other => other,
    }
}

/// Whether the `|` at `i` opens a closure's parameters rather than being an operator or a
/// pattern alternative, judged by the token before it.
fn opens_closure(seq: &[TokenTree], i: usize) -> bool {
    match i.checked_sub(1).map(|before| &seq[before]) {
        None => true,
        Some(TokenTree::Punct(p)) => p.as_char() != '?',
        Some(TokenTree::Ident(word)) => ["move", "async", "return"].iter().any(|k| word == k),
        Some(TokenTree::Group(list)) => {
            list.delimiter() == Delimiter::Bracket
                && matches!(i.checked_sub(2).map(|m| &seq[m]), Some(TokenTree::Ident(m)) if m == "move")
        }
        Some(TokenTree::Literal(_)) => false,
    }
}

fn thrown(span: Span, form: Form, value: &[TokenTree]) -> TokenStream {
    let error = match form {
        Form::Anyhow => quote_spanned!(span=> ::rustx::anyhow::anyhow!(#(#value)*)),
        Form::Into => quote_spanned!(span=> ::core::convert::Into::into(#(#value)*)),
    };
    quote_spanned!(span=> return ::core::result::Result::Err(#error))
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::{ToTokens, quote};

    /// The whole token phase on `sugar`, reprinted, or its error messages joined by `|`.
    fn expand(sugar: TokenStream) -> String {
        match crate::try_expand(sugar) {
            Ok(file) => file.into_token_stream().to_string(),
            Err(err) => err
                .into_iter()
                .map(|err| err.to_string())
                .collect::<Vec<_>>()
                .join("|"),
        }
    }

    fn reprint(plain: TokenStream) -> String {
        syn::parse2::<syn::File>(plain)
            .map(|file| file.into_token_stream().to_string())
            .unwrap_or_default()
    }

    #[test]
    fn throw_in_a_plain_throws_fn_returns_an_anyhow_error() {
        assert_eq!(
            expand(quote! {
                fn load(workers: u8) -> u8 throws {
                    if workers == 0 { throw "workers must be > 0"; }
                    workers
                }
            }),
            reprint(quote! {
                fn load(workers: u8) -> ::rustx::anyhow::Result<u8> {
                    if workers == 0 {
                        return ::core::result::Result::Err(
                            ::rustx::anyhow::anyhow!("workers must be > 0")
                        );
                    }
                    ::core::result::Result::Ok(workers)
                }
            })
        );
    }

    #[test]
    fn throw_with_an_error_type_converts_through_into() {
        assert_eq!(
            expand(quote! {
                fn parse(text: &str) -> u32 throws ParseError {
                    if text.is_empty() { throw ParseError::Empty }
                    text.parse().map_err(ParseError::Bad)?
                }
            }),
            reprint(quote! {
                fn parse(text: &str) -> ::core::result::Result<u32, ParseError> {
                    if text.is_empty() {
                        return ::core::result::Result::Err(
                            ::core::convert::Into::into(ParseError::Empty)
                        )
                    }
                    ::core::result::Result::Ok(text.parse().map_err(ParseError::Bad)?)
                }
            })
        );
    }

    #[test]
    fn throw_is_recognised_after_arrows_assignments_and_attributes() {
        assert_eq!(
            expand(quote! {
                fn f(o: Option<u8>) -> u8 throws E {
                    let n = match o { Some(n) => n, None => throw E::Missing, };
                    let m: u8 = throw E::Never;
                    #[allow(unreachable_code)] throw 7
                }
            }),
            reprint(quote! {
                fn f(o: Option<u8>) -> ::core::result::Result<u8, E> {
                    let n = match o {
                        Some(n) => n,
                        None => return ::core::result::Result::Err(
                            ::core::convert::Into::into(E::Missing)
                        ),
                    };
                    let m: u8 = return ::core::result::Result::Err(
                        ::core::convert::Into::into(E::Never)
                    );
                    #[allow(unreachable_code)]
                    return ::core::result::Result::Err(::core::convert::Into::into(7))
                }
            })
        );
    }

    #[test]
    fn the_thrown_value_runs_to_the_end_of_its_statement_arm_or_group() {
        assert_eq!(
            expand(quote! {
                fn f(a: u8) -> () throws E {
                    throw a + Wrap::<A, B>::new(a, 1).x?;
                    g(match a { 0 => throw a.into(), _ => 1 });
                }
            }),
            reprint(quote! {
                fn f(a: u8) -> ::core::result::Result<(), E> {
                    return ::core::result::Result::Err(
                        ::core::convert::Into::into(a + Wrap::<A, B>::new(a, 1).x?)
                    );
                    g(match a {
                        0 => return ::core::result::Result::Err(
                            ::core::convert::Into::into(a.into())
                        ),
                        _ => 1
                    });
                    ::core::result::Result::Ok(())
                }
            })
        );
    }

    #[test]
    fn a_nested_throws_fn_throws_its_own_error_type() {
        assert_eq!(
            expand(quote! {
                fn outer() -> () throws {
                    fn inner() -> () throws E { throw E::A; }
                    throw "outer";
                }
            }),
            reprint(quote! {
                fn outer() -> ::rustx::anyhow::Result<()> {
                    fn inner() -> ::core::result::Result<(), E> {
                        return ::core::result::Result::Err(::core::convert::Into::into(E::A));
                    }
                    return ::core::result::Result::Err(::rustx::anyhow::anyhow!("outer"));
                }
            })
        );
    }

    #[test]
    fn closures_match_arms_and_method_bodies_keep_the_form_where_they_should() {
        assert_eq!(
            expand(quote! {
                impl S {
                    fn m(&self, xs: &[u8]) -> u8 throws E {
                        let n = xs.iter().map(|x| x + 1).sum::<u8>();
                        match n { A | B => throw E::Bad, _ => {} }
                        if self.ok || n > 0 { n } else { throw E::Zero }
                    }
                }
            }),
            reprint(quote! {
                impl S {
                    fn m(&self, xs: &[u8]) -> ::core::result::Result<u8, E> {
                        let n = xs.iter().map(|x| x + 1).sum::<u8>();
                        match n {
                            A | B => return ::core::result::Result::Err(
                                ::core::convert::Into::into(E::Bad)
                            ),
                            _ => {}
                        }
                        ::core::result::Result::Ok(if self.ok || n > 0 {
                            n
                        } else {
                            return ::core::result::Result::Err(::core::convert::Into::into(E::Zero))
                        })
                    }
                }
            })
        );
    }

    #[test]
    fn throw_outside_a_throws_fn_is_an_error() {
        assert_eq!(expand(quote! { fn f() { throw "no"; } }), OUTSIDE);
    }

    #[test]
    fn throw_in_a_nested_fn_without_throws_is_an_error() {
        assert_eq!(
            expand(quote! { fn f() -> () throws { fn g() { throw "no"; } } }),
            OUTSIDE
        );
    }

    #[test]
    fn throw_in_closures_and_async_and_const_blocks_is_an_error() {
        let errors = expand(quote! {
            fn f() -> () throws {
                let a = || throw "a";
                let b = |x: u8| { throw "b" };
                let c = move |x| -> u8 { if x { throw "c" } else { 1 } };
                let d = async { throw "d" };
                let e = async move { throw "e" };
                let g = const { throw "g" };
                let h = move[v.clone()] || throw "h";
                let i = |x| match x { _ => throw "i" };
            }
        });
        assert_eq!(errors, [OUTSIDE; 8].join("|"));
    }

    #[test]
    fn after_a_closure_ends_throw_belongs_to_the_fn_again() {
        assert_eq!(
            expand(quote! {
                fn f() -> () throws E { let g = |x| x; throw E::A; }
            }),
            reprint(quote! {
                fn f() -> ::core::result::Result<(), E> {
                    let g = |x| x;
                    return ::core::result::Result::Err(::core::convert::Into::into(E::A));
                }
            })
        );
    }

    #[test]
    fn every_misplaced_throw_is_reported() {
        let errors = expand(quote! { fn f() { throw "a"; } fn g() { throw "b"; } });
        assert_eq!(errors, [OUTSIDE; 2].join("|"));
    }

    #[test]
    fn throw_as_an_identifier_passes_through() {
        let valid = quote! {
            fn throw(throw: u8) -> u8 { throw }
            struct S { throw: u8 }
            fn g(s: S, throw: u8, v: &[u8]) -> u8 {
                let throw = 1;
                throw.f();
                throw(1);
                throw::m::E;
                throw & 1;
                throw as u8;
                r#throw;
                let Some(x) = throw else { return 0 };
                match throw { throw if throw > 0 => throw, _ => 0 };
                for throw in v {}
                S { throw };
                s.throw
            }
        };
        assert_eq!(expand(valid.clone()), reprint(valid));
    }

    #[test]
    fn only_the_throws_marker_makes_a_throws_body() {
        let form = |body: TokenStream| match body.into_iter().next() {
            Some(TokenTree::Group(group)) => {
                marked_form(&group).map(|form| matches!(form, Form::Anyhow))
            }
            _ => None,
        };
        assert_eq!(
            form(quote!(
                {
                    #![__rustx_throws(anyhow)]
                    x
                }
            )),
            Some(true)
        );
        assert_eq!(
            form(quote!(
                {
                    #![__rustx_throws]
                    x
                }
            )),
            Some(false)
        );
        assert_eq!(
            form(quote!(
                {
                    #![__rustx_throws(other)]
                    x
                }
            )),
            None
        );
        assert_eq!(
            form(quote!(
                {
                    #![allow(unused)]
                    x
                }
            )),
            None
        );
        assert_eq!(form(quote!({ x })), None);
    }
}
