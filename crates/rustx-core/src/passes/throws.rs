//! `T throws` return types and tail `Ok(..)` (problem 10).
//!
//! Two halves, because the sugar does not parse but the tail needs the parsed body:
//!
//! 1. Token pass ([`rewrite`]): in a fn signature, `-> T throws` becomes
//!    `-> ::rustx::anyhow::Result<T>` and `-> T throws E` becomes
//!    `-> ::core::result::Result<T, E>`. A body, when there is one, starts with the marker
//!    `#![__rustx_throws]`, written `#![__rustx_throws(anyhow)]` for plain `throws`, so the
//!    `throw` pass knows which of the two error types it builds.
//! 2. AST pass ([`lower`]): every marked body loses its marker and gets its tail wrapped in
//!    `::core::result::Result::Ok(..)`:
//!
//! ```text
//! fn load(path: &str) -> Settings throws { let text = read(path)?; parse(&text) }
//! fn load(path: &str) -> ::rustx::anyhow::Result<Settings> {
//!     let text = read(path)?;
//!     ::core::result::Result::Ok(parse(&text))
//! }
//! ```
//!
//! `anyhow` is reached through the `rustx` facade, so users do not depend on it themselves.
//! Generated tokens take the span of `throws`.
//!
//! # Detection
//!
//! The pass runs after `prefix_types`, so the value and error types are plain Rust by then
//! (`iter &str throws` is `impl Iterator<Item = &str> + '_ throws`).
//!
//! `throws` is sugar only in the return type of `fn name<..>(..) -> ..`, outside `<..>`, after a
//! token that can end a type. So a type named `throws` (`-> throws`, `-> m::throws`,
//! `-> &'a throws`, `-> Vec<throws>`) stays as written, and closures and fn-pointer types never
//! take the sugar; `r#throws` always means the identifier. `fn f() throws` is an error: the
//! problem shows only `-> T throws`, and `-> () throws` says the same thing explicitly.
//!
//! The marker goes inside the body rather than on the item, because the body is right after the
//! return type while an item's start lies behind its visibility and qualifiers. A declaration
//! without a body only gets the type: each implementation says `throws` itself.
//!
//! # Tail
//!
//! The tail is wrapped once, as a whole: the problem shows no `if` or `match` tails, and arms
//! that build their own `Ok(..)` would otherwise be wrapped twice. Left alone are a tail that is
//! already a call of a path ending in `Ok` or `Err`, and a tail or last statement that produces
//! no value: `return`, `panic!`, `unreachable!`, `todo!`, `unimplemented!` and a `loop` that no
//! `break` leaves. A body without a tail, or ending in `for`, `while` or an `if` without `else`
//! (which are `()` anyway), gets `Ok(())` appended. Closures and nested fns keep their own
//! signatures: only the marked fn's own block is touched.
//!
//! [`MARKER`] stays on the fn until this pass, the last one, takes it off, so a pass for
//! `throw e` that runs before it can tell a `throws` body from any other.
//!
//! # Limitations
//!
//! - Signatures inside macro bodies get the new type but are not lowered: the marker stays and
//!   rustc reports the unknown attribute `__rustx_throws` at `throws`.
//! - A `break` that leaves a `loop` only from inside a macro invocation is not seen, so that
//!   loop counts as producing no value and is not wrapped.

use std::mem;

use proc_macro2::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};
use quote::quote_spanned;
use syn::visit_mut::{self, VisitMut};
use syn::{Block, Expr, ExprIf, ExprLoop, Macro, Stmt, Type};

use super::signatures::for_each_fn;
use crate::tokens::angle::angle_depth;
use crate::tokens::map_sequences_reporting;
use crate::tokens::params::parameter_list;
use crate::tokens::punct::{is_arrow, is_punct, loosen_end};
use crate::tokens::syntax::{combine, take_attr};

/// Name of the marker attribute at the start of a `throws` body; [`lower`] takes it off.
pub(super) const MARKER: &str = "__rustx_throws";

/// The marker's argument on a body whose error type is `anyhow::Error`.
pub(super) const ANYHOW: &str = "anyhow";

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    map_sequences_reporting(tokens, rewrite_sequence)
}

pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    for_each_fn(file, |attrs, _, body| {
        match (take_attr(attrs, MARKER), body) {
            (Some(marker), Some(body)) => wrap_tail(body, marker.pound_token.spans[0]),
            _ => Ok(()),
        }
    })
}

fn rewrite_sequence(seq: &[TokenTree], errors: &mut Option<syn::Error>) -> Vec<TokenTree> {
    let mut out = Vec::with_capacity(seq.len());
    let mut copied = 0;
    for i in 0..seq.len() {
        if i < copied {
            continue;
        }
        let Some(params) = parameter_list(seq, i) else {
            continue;
        };
        match rewrite_signature(seq, params + 1) {
            Ok(Some((end, rewritten))) => {
                out.extend_from_slice(&seq[copied..=params]);
                out.extend(rewritten);
                copied = end;
            }
            Ok(None) => {}
            Err(err) => combine(errors, err),
        }
    }
    out.extend_from_slice(&seq[copied..]);
    out
}

/// For a signature whose parameter group ends before `after`: when it returns `T throws`, the
/// tokens that replace `seq[after..end]`, with `end`.
fn rewrite_signature(
    seq: &[TokenTree],
    after: usize,
) -> syn::Result<Option<(usize, Vec<TokenTree>)>> {
    if let Some(TokenTree::Ident(word)) = seq.get(after)
        && word == "throws"
    {
        let message = "`throws` needs a return type before it: `-> T throws`, or `-> () throws` \
                       for no value";
        return Err(syn::Error::new(word.span(), message));
    }
    if after + 1 >= seq.len() || !is_arrow(seq, after + 1) {
        return Ok(None);
    }
    let start = after + 2;
    let mut stop = start;
    let mut depth = 0;
    let mut sugar = Vec::new();
    while stop < seq.len() && !(depth == 0 && ends_return_type(&seq[stop])) {
        if depth == 0 && stop > start && is_throws(&seq[stop]) && !type_continues(seq, stop) {
            sugar.push(stop);
        }
        depth = angle_depth(seq, stop, depth);
        stop += 1;
    }
    let Some((&at, again)) = sugar.split_first() else {
        return Ok(None);
    };
    if let Some(&second) = again.first() {
        let message = "`throws` is written twice; a return type takes it once";
        return Err(syn::Error::new(seq[second].span(), message));
    }
    let span = seq[at].span();
    let ok = loosen_end(seq[start..at].iter().cloned());
    let error = &seq[at + 1..stop];
    let result = if error.is_empty() {
        quote_spanned!(span=> ::rustx::anyhow::Result<#ok>)
    } else {
        check_error_type(error, span)?;
        let error = loosen_end(error.iter().cloned());
        quote_spanned!(span=> ::core::result::Result<#ok, #error>)
    };
    let mut out = seq[after..start].to_vec();
    out.extend(result);
    match body_after(seq, stop) {
        Some((body, group)) => {
            out.extend_from_slice(&seq[stop..body]);
            out.push(marked(group, span, error.is_empty()));
            Ok(Some((body + 1, out)))
        }
        None => Ok(Some((stop, out))),
    }
}

fn is_throws(tree: &TokenTree) -> bool {
    matches!(tree, TokenTree::Ident(word) if word == "throws")
}

/// Whether `tree`, outside `<..>`, ends a return type: a `where` clause, the body or the `;` of
/// a declaration.
fn ends_return_type(tree: &TokenTree) -> bool {
    match tree {
        TokenTree::Ident(word) => word == "where",
        TokenTree::Group(group) => group.delimiter() == Delimiter::Brace,
        punct => is_punct(punct, ';'),
    }
}

/// Whether the token before `at` leaves a type unfinished, so a `throws` at `at` is part of it.
fn type_continues(seq: &[TokenTree], at: usize) -> bool {
    match &seq[at - 1] {
        TokenTree::Punct(p) => {
            matches!(p.as_char(), ':' | '&' | '*' | '+' | '\'') || is_arrow(seq, at - 1)
        }
        TokenTree::Ident(word) => {
            if at >= 2 && is_punct(&seq[at - 2], '\'') {
                // `&'a throws` names a type; `dyn Tr + 'a throws` ends with the lifetime.
                return at >= 3 && is_punct(&seq[at - 3], '&');
            }
            ["dyn", "impl", "mut", "const"]
                .iter()
                .any(|keyword| word == keyword)
        }
        _ => false,
    }
}

fn check_error_type(error: &[TokenTree], span: Span) -> syn::Result<()> {
    if syn::parse2::<Type>(error.iter().cloned().collect()).is_ok() {
        return Ok(());
    }
    let message = "expected an error type after `throws`, as in `-> T throws MyError`";
    Err(syn::Error::new(span, message))
}

/// The fn body from `from` on, past a `where` clause; `None` for a declaration ending in `;`.
fn body_after(seq: &[TokenTree], from: usize) -> Option<(usize, &Group)> {
    let mut depth = 0;
    for (at, tree) in seq.iter().enumerate().skip(from) {
        if depth == 0 {
            match tree {
                TokenTree::Group(group) if group.delimiter() == Delimiter::Brace => {
                    return Some((at, group));
                }
                punct if is_punct(punct, ';') => return None,
                _ => {}
            }
        }
        depth = angle_depth(seq, at, depth);
    }
    None
}

fn marked(body: &Group, span: Span, anyhow: bool) -> TokenTree {
    let name = Ident::new(MARKER, span);
    let mut stream = if anyhow {
        let form = Ident::new(ANYHOW, span);
        quote_spanned!(span=> #![#name(#form)])
    } else {
        quote_spanned!(span=> #![#name])
    };
    stream.extend(body.stream());
    let mut rebuilt = Group::new(Delimiter::Brace, stream);
    rebuilt.set_span(body.span());
    TokenTree::Group(rebuilt)
}

fn wrap_tail(body: &mut Block, span: Span) -> syn::Result<()> {
    match body.stmts.last_mut() {
        Some(Stmt::Expr(tail, None)) if !is_unit(tail) => {
            if !(produces_no_value(tail) || is_ok_or_err(tail)) {
                let value = mem::replace(tail, Expr::PLACEHOLDER);
                *tail = ok(span, Some(value))?;
            }
        }
        Some(Stmt::Expr(last, Some(_))) if produces_no_value(last) => {}
        Some(Stmt::Macro(last)) if diverging_macro(&last.mac) => {}
        _ => body.stmts.push(Stmt::Expr(ok(span, None)?, None)),
    }
    Ok(())
}

/// `::core::result::Result::Ok(value)`, or `Ok(())` without a value. The value is moved in, not
/// reprinted, so its tokens and spans stay the user's.
fn ok(span: Span, value: Option<Expr>) -> syn::Result<Expr> {
    let mut call: syn::ExprCall =
        syn::parse2(quote_spanned!(span=> ::core::result::Result::Ok(())))?;
    if let (Some(value), Some(arg)) = (value, call.args.first_mut()) {
        *arg = value;
    }
    Ok(Expr::Call(call))
}

/// Tails whose type is always `()`, so `Ok(())` after them reads better than `Ok(for ..)`.
fn is_unit(tail: &Expr) -> bool {
    matches!(
        tail,
        Expr::ForLoop(_)
            | Expr::While(_)
            | Expr::If(ExprIf {
                else_branch: None,
                ..
            })
    )
}

fn is_ok_or_err(tail: &Expr) -> bool {
    let Expr::Call(call) = tail else {
        return false;
    };
    matches!(&*call.func, Expr::Path(path)
        if path.path.segments.last().is_some_and(|last| last.ident == "Ok" || last.ident == "Err"))
}

fn produces_no_value(expr: &Expr) -> bool {
    match expr {
        Expr::Return(_) => true,
        Expr::Macro(mac) => diverging_macro(&mac.mac),
        Expr::Loop(looped) => !left_by_break(looped),
        _ => false,
    }
}

fn diverging_macro(mac: &Macro) -> bool {
    mac.path.segments.last().is_some_and(|last| {
        ["panic", "unreachable", "todo", "unimplemented"]
            .iter()
            .any(|name| last.ident == name)
    })
}

/// Whether a `break` in the body of `looped` leaves it, so the loop produces a value.
fn left_by_break(looped: &ExprLoop) -> bool {
    let mut breaks = Breaks {
        label: looped.label.as_ref().map(|label| label.name.ident.clone()),
        inner_loops: 0,
        found: false,
    };
    // `syn` is built with `visit-mut` only, so the search walks a copy rather than adding the
    // `visit` feature for one read-only walk.
    breaks.visit_block_mut(&mut looped.body.clone());
    breaks.found
}

/// Finds a `break` that targets one loop; it changes nothing.
struct Breaks {
    label: Option<Ident>,
    inner_loops: usize,
    found: bool,
}

impl Breaks {
    fn inner(&mut self, visit: impl FnOnce(&mut Self)) {
        self.inner_loops += 1;
        visit(self);
        self.inner_loops -= 1;
    }
}

impl VisitMut for Breaks {
    fn visit_expr_break_mut(&mut self, node: &mut syn::ExprBreak) {
        self.found |= match &node.label {
            None => self.inner_loops == 0,
            Some(label) => self.label.as_ref() == Some(&label.ident),
        };
        visit_mut::visit_expr_break_mut(self, node);
    }

    fn visit_expr_loop_mut(&mut self, node: &mut ExprLoop) {
        self.inner(|this| visit_mut::visit_expr_loop_mut(this, node));
    }

    fn visit_expr_while_mut(&mut self, node: &mut syn::ExprWhile) {
        self.inner(|this| visit_mut::visit_expr_while_mut(this, node));
    }

    fn visit_expr_for_loop_mut(&mut self, node: &mut syn::ExprForLoop) {
        self.inner(|this| visit_mut::visit_expr_for_loop_mut(this, node));
    }

    // A `break` cannot leave a closure, an async or const block, or a nested item.
    fn visit_expr_closure_mut(&mut self, _: &mut syn::ExprClosure) {}

    fn visit_expr_async_mut(&mut self, _: &mut syn::ExprAsync) {}

    fn visit_expr_const_mut(&mut self, _: &mut syn::ExprConst) {}

    fn visit_item_mut(&mut self, _: &mut syn::Item) {}
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::{ToTokens, quote};

    fn compact(tokens: &TokenStream) -> String {
        tokens.to_string().split_whitespace().collect()
    }

    /// The token half alone, compacted, or its error message.
    fn marks(sugar: &str) -> String {
        rewrite(sugar.parse().unwrap_or_default())
            .map_or_else(|err| err.to_string(), |out| compact(&out))
    }

    fn same(source: &str) -> String {
        compact(&source.parse().unwrap_or_default())
    }

    fn desugar(sugar: TokenStream) -> syn::Result<String> {
        let mut file: syn::File = syn::parse2(rewrite(sugar)?)?;
        lower(&mut file)?;
        Ok(file.into_token_stream().to_string())
    }

    fn reprint(plain: TokenStream) -> String {
        syn::parse2::<syn::File>(plain)
            .map(|file| file.into_token_stream().to_string())
            .unwrap_or_default()
    }

    #[test]
    fn t_throws_becomes_an_anyhow_result_and_marks_the_body_as_anyhow() {
        assert_eq!(
            marks("pub fn load(path: &str) -> Settings throws { read(path) }"),
            same(
                "pub fn load(path: &str) -> ::rustx::anyhow::Result<Settings> \
                 { #![__rustx_throws(anyhow)] read(path) }"
            )
        );
    }

    #[test]
    fn t_throws_e_becomes_a_core_result() {
        assert_eq!(
            marks("fn f<T>() -> Vec<T> throws Box<dyn Error> where T: Clone { g() }"),
            same(
                "fn f<T>() -> ::core::result::Result<Vec<T>, Box<dyn Error> > where T: Clone \
                 { #![__rustx_throws] g() }"
            )
        );
    }

    #[test]
    fn a_declaration_without_a_body_only_gets_the_type() {
        assert_eq!(
            marks("trait T { fn f(&self) -> u8 throws; fn g() -> () throws E; }"),
            same(
                "trait T { fn f(&self) -> ::rustx::anyhow::Result<u8>; \
                 fn g() -> ::core::result::Result<(), E>; }"
            )
        );
    }

    #[test]
    fn nested_fns_and_methods_are_rewritten_on_their_own() {
        assert_eq!(
            marks("impl S { fn m(&self) -> u8 throws { fn n() -> u16 throws E { 2 } 1 } }"),
            same(
                "impl S { fn m(&self) -> ::rustx::anyhow::Result<u8> { #![__rustx_throws(anyhow)] \
                 fn n() -> ::core::result::Result<u16, E> { #![__rustx_throws] 2 } 1 } }"
            )
        );
    }

    #[test]
    fn throws_as_an_identifier_or_type_name_is_unchanged() {
        let valid = quote! {
            fn throws(throws: u8) -> throws { let throws = 1; throws }
            fn a<'a>() -> &'a throws {}
            fn b() -> m::throws {}
            fn c() -> Vec<throws> {}
            fn d() -> &mut throws {}
            fn e() -> r#throws {}
            fn f() -> impl Fn() -> throws {}
            struct S { throws: fn() -> throws }
            fn g() { let c = |x| -> throws { x }; }
        };
        assert_eq!(
            rewrite(valid.clone()).map(|out| out.to_string()).ok(),
            Some(valid.to_string())
        );
    }

    #[test]
    fn throws_without_a_return_type_is_an_error() {
        assert_eq!(
            marks("fn f() throws {}"),
            "`throws` needs a return type before it: `-> T throws`, or `-> () throws` for no value"
        );
    }

    #[test]
    fn throws_twice_is_an_error() {
        let message = "`throws` is written twice; a return type takes it once";
        assert_eq!(marks("fn f() -> u8 throws throws {}"), message);
        assert_eq!(marks("fn f() -> u8 throws E throws {}"), message);
    }

    #[test]
    fn throws_followed_by_a_non_type_is_an_error() {
        let message = "expected an error type after `throws`, as in `-> T throws MyError`";
        assert_eq!(marks("fn f() -> u8 throws 42 {}"), message);
        assert_eq!(marks("fn f() -> u8 throws = E {}"), message);
    }

    #[test]
    fn errors_from_several_signatures_are_reported_together() {
        let err = rewrite(quote! { fn f() throws {} fn g() -> u8 throws 1 {} })
            .err()
            .map(|err| err.into_iter().count());
        assert_eq!(err, Some(2));
    }

    #[test]
    fn the_tail_is_wrapped_in_ok_and_the_marker_removed() {
        assert_eq!(
            desugar(quote! { fn f(x: &str) -> u8 throws { let n = x.parse()?; n + 1 } }).ok(),
            Some(reprint(quote! {
                fn f(x: &str) -> ::rustx::anyhow::Result<u8> {
                    let n = x.parse()?;
                    ::core::result::Result::Ok(n + 1)
                }
            }))
        );
    }

    #[test]
    fn ok_and_err_tails_are_left_alone() {
        let plain = quote! {
            fn a() -> ::core::result::Result<u8, E> { Ok(1) }
            fn b() -> ::core::result::Result<u8, E> { Err(E) }
            fn c() -> ::core::result::Result<u8, E> { std::result::Result::Ok::<u8, E>(1) }
        };
        let sugar = quote! {
            fn a() -> u8 throws E { Ok(1) }
            fn b() -> u8 throws E { Err(E) }
            fn c() -> u8 throws E { std::result::Result::Ok::<u8, E>(1) }
        };
        assert_eq!(desugar(sugar).ok(), Some(reprint(plain)));
    }

    #[test]
    fn tails_and_last_statements_without_a_value_are_left_alone() {
        let bodies = [
            quote!(return Ok(1)),
            quote!(return Ok(1);),
            quote!(panic!("no")),
            quote!(std::unreachable!();),
            quote!(todo!()),
            quote!(unimplemented!()),
            quote!(loop {
                tick();
            }),
            quote!('outer: loop {
                loop {
                    break;
                }
                let c = || loop {
                    break 1;
                };
            }),
        ];
        for body in bodies {
            assert_eq!(
                desugar(quote! { fn f() -> u8 throws { #body } }).ok(),
                Some(reprint(
                    quote! { fn f() -> ::rustx::anyhow::Result<u8> { #body } }
                )),
                "{body}"
            );
        }
    }

    #[test]
    fn a_loop_left_by_break_is_wrapped() {
        for body in [
            quote!(loop {
                break 1;
            }),
            quote!('outer: loop {
                loop {
                    break 'outer 1;
                }
            }),
            quote!(loop {
                if done() {
                    break;
                }
            }),
        ] {
            assert_eq!(
                desugar(quote! { fn f() -> u8 throws { #body } }).ok(),
                Some(reprint(quote! {
                    fn f() -> ::rustx::anyhow::Result<u8> { ::core::result::Result::Ok(#body) }
                })),
                "{body}"
            );
        }
    }

    #[test]
    fn a_body_without_a_value_tail_gets_ok_unit() {
        for body in [
            quote!(),
            quote!(run()?;),
            quote!(let x = 1;),
            quote!(for x in xs {
                run(x)?;
            }),
            quote!(while more() {
                step()?;
            }),
            quote!(if ready() {
                go()?;
            }),
        ] {
            assert_eq!(
                desugar(quote! { fn f() -> () throws { #body } }).ok(),
                Some(reprint(quote! {
                    fn f() -> ::rustx::anyhow::Result<()> { #body ::core::result::Result::Ok(()) }
                })),
                "{body}"
            );
        }
    }

    #[test]
    fn closures_and_nested_fns_keep_their_own_tails() {
        assert_eq!(
            desugar(quote! {
                fn f() -> u8 throws {
                    fn g() -> u8 { 1 }
                    let c = || 2;
                    g() + c()
                }
            })
            .ok(),
            Some(reprint(quote! {
                fn f() -> ::rustx::anyhow::Result<u8> {
                    fn g() -> u8 { 1 }
                    let c = || 2;
                    ::core::result::Result::Ok(g() + c())
                }
            }))
        );
    }

    #[test]
    fn methods_and_trait_defaults_are_lowered() {
        assert_eq!(
            desugar(quote! {
                trait T { fn need(&self) -> u8 throws; fn get(&self) -> u8 throws { self.need()? } }
                impl S { fn m(&self) -> String throws E { self.name.clone() } }
            })
            .ok(),
            Some(reprint(quote! {
                trait T {
                    fn need(&self) -> ::rustx::anyhow::Result<u8>;
                    fn get(&self) -> ::rustx::anyhow::Result<u8> {
                        ::core::result::Result::Ok(self.need()?)
                    }
                }
                impl S {
                    fn m(&self) -> ::core::result::Result<String, E> {
                        ::core::result::Result::Ok(self.name.clone())
                    }
                }
            }))
        );
    }

    #[test]
    fn prefix_types_in_the_value_and_error_types_are_expanded_first() {
        let expanded = crate::try_expand(quote! {
            fn lines(&self) -> iter &str throws { self.text.lines() }
            fn shape() -> rc Node throws ~Error { node() }
        })
        .map(|file| file.into_token_stream().to_string());
        assert_eq!(
            expanded.ok(),
            Some(reprint(quote! {
                fn lines(&self) -> ::rustx::anyhow::Result<
                    impl ::core::iter::Iterator<Item = &str> + '_
                > {
                    ::core::result::Result::Ok(self.text.lines())
                }
                fn shape() -> ::core::result::Result<
                    ::std::rc::Rc<::core::cell::RefCell<Node> >,
                    ::std::boxed::Box<dyn Error + ::core::marker::Send + ::core::marker::Sync>
                > {
                    ::core::result::Result::Ok(node())
                }
            }))
        );
    }

    #[test]
    fn throws_works_next_to_parameter_sugar() {
        let expanded = crate::try_expand(quote! {
            fn f(a: T: Clone, name: into String) -> T throws { a }
        })
        .map(|file| file.into_token_stream().to_string());
        assert_eq!(
            expanded.ok(),
            Some(reprint(quote! {
                fn f<T>(a: T, name: impl ::core::convert::Into<String>) -> ::rustx::anyhow::Result<T>
                where T: Clone {
                    let name: String = ::core::convert::Into::into(name);
                    ::core::result::Result::Ok(a)
                }
            }))
        );
    }

    #[test]
    fn valid_rust_passes_through_unchanged() {
        let valid = quote! {
            fn throws() -> Result<u8, E> { let throws = 1; Ok(throws) }
            trait T { fn f(&self) -> u8; }
        };
        assert_eq!(desugar(valid.clone()).ok(), Some(reprint(valid)));
    }
}
