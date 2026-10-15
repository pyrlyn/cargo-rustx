//! `shared` bindings (problem 12, `problems/12-arc-mutex.md`).
//!
//! `shared s = e;` declares a value shared between owners and threads. `shared T` in a type
//! position is `prefix_types`' business (`::rustx::Shared<T>`), since it is detected like the
//! other type prefixes.
//!
//! A token pass, because `shared s = e;` is not a Rust statement: where `shared` starts a
//! statement (the `var` rules, `tokens::position::starts_statement`) and a name follows, it
//! becomes `#[__rustx_shared] let s = e;`. `shared` is an ordinary identifier everywhere else, so
//! `shared = 1`, `shared.f`, `shared(1)`, `shared as u8` and `r#shared x` stay as written.
//! `shared mut s`, `shared ref s`, `shared _` and patterns other than one name (with an
//! optional `: T`) are errors.
//!
//! The AST pass takes the marker off and picks the handle from how the scope uses the binding
//! (`access::scan`), because the problem file makes read-only data a plain `Arc`:
//!
//! - `::std::sync::Arc::new(e)` when every use is a read access (`s.f`, `s.len()`, nothing
//!   assigned, borrowed `&mut` or passed to a known `&mut self` std method) and the handle
//!   itself is never used (no `lock` / `with` / `with_mut` / `is_poisoned`, no bare `s`, no
//!   `s.clone()` other than `let s = s.clone();`). An `Arc` of a value nothing writes needs no
//!   lock, and reads reach the value through `Deref`.
//! - `::rustx::Shared::new(e)` otherwise, with every access going through `s.lock()` as
//!   `access` describes.
//!
//! A written type is the value's: `shared s: T = e` annotates the handle as `Shared<T>` or
//! `Arc<T>`. A `shared` binding inside a macro call is an error: the AST pass cannot see into the
//! call to rewrite its scope.

use proc_macro2::{Delimiter, Group, Ident, Punct, Spacing, Span, TokenStream, TokenTree};
use syn::visit_mut::{self, VisitMut};
use syn::{Block, Expr, Local, Macro, Pat, Stmt, parse_quote_spanned};

use super::access::{self, Opening};
use crate::tokens::map_sequences_reporting;
use crate::tokens::position::starts_statement;
use crate::tokens::punct::{is_alone, is_punct, is_single_colon};
use crate::tokens::syntax::{combine, take_attr};

const MARKER: &str = "__rustx_shared";

const LOCK: Opening = Opening {
    open: lock,
    handle_methods: &["lock", "with", "with_mut", "is_poisoned"],
};

// A mutex has no read-only guard, so whether the accesses write does not matter here.
fn lock(name: &Ident, _writes: bool, guard: bool) -> Expr {
    if guard {
        parse_quote_spanned!(name.span()=> &mut *#name.lock())
    } else {
        parse_quote_spanned!(name.span()=> #name.lock())
    }
}

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    map_sequences_reporting(tokens, rewrite_sequence)
}

fn rewrite_sequence(seq: &[TokenTree], errors: &mut Option<syn::Error>) -> Vec<TokenTree> {
    let mut out = Vec::with_capacity(seq.len() + 2);
    for (i, tree) in seq.iter().enumerate() {
        if let TokenTree::Ident(shared) = tree
            && shared == "shared"
            && starts_statement(seq, i)
        {
            match classify(shared.span(), &seq[i + 1..]) {
                Ok(true) => {
                    out.extend(marked_let(shared.span()));
                    continue;
                }
                Ok(false) => {}
                Err(err) => combine(errors, err),
            }
        }
        out.push(tree.clone());
    }
    out
}

/// Whether a `shared` at statement start declares a binding (`Ok(true)`), is the identifier
/// (`Ok(false)`), or is sugar written wrong.
fn classify(shared: Span, after: &[TokenTree]) -> syn::Result<bool> {
    let single_name = || {
        syn::Error::new(
            shared,
            "`shared` takes a single name, as in `shared name = value;`",
        )
    };
    match after {
        [TokenTree::Ident(name), rest @ ..] => match name.to_string().as_str() {
            "as" => Ok(false),
            "mut" => Err(syn::Error::new(
                name.span(),
                "a `shared` value changes through its lock, so the binding needs no `mut`; \
                 remove it",
            )),
            "ref" | "_" => Err(single_name()),
            _ => match rest.first() {
                // A tuple-struct, struct, path or `@` pattern rather than a name.
                Some(TokenTree::Group(g)) if g.delimiter() != Delimiter::Bracket => {
                    Err(single_name())
                }
                Some(tree)
                    if is_punct(tree, '@')
                        || (is_punct(tree, ':') && !is_single_colon(rest, 0)) =>
                {
                    Err(single_name())
                }
                _ => Ok(true),
            },
        },
        [TokenTree::Group(g), next, ..]
            if g.delimiter() == Delimiter::Parenthesis
                && (is_alone(next, '=') || is_punct(next, ':')) =>
        {
            Err(single_name())
        }
        _ => Ok(false),
    }
}

/// `#[__rustx_shared] let`, all with the span of `shared`.
fn marked_let(span: Span) -> [TokenTree; 3] {
    let mut hash = Punct::new('#', Spacing::Alone);
    hash.set_span(span);
    let marker = TokenTree::Ident(Ident::new(MARKER, span));
    let mut attr = Group::new(Delimiter::Bracket, marker.into());
    attr.set_span(span);
    [
        TokenTree::Punct(hash),
        TokenTree::Group(attr),
        TokenTree::Ident(Ident::new("let", span)),
    ]
}

pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    let mut lowering = Lowering { errors: None };
    lowering.visit_file_mut(file);
    lowering.errors.map_or(Ok(()), Err)
}

struct Lowering {
    errors: Option<syn::Error>,
}

impl VisitMut for Lowering {
    fn visit_block_mut(&mut self, block: &mut Block) {
        for i in 0..block.stmts.len() {
            let (head, scope) = block.stmts.split_at_mut(i + 1);
            if let Stmt::Local(local) = &mut head[i]
                && let Some(marker) = take_attr(&mut local.attrs, MARKER)
            {
                let span = marker
                    .path()
                    .get_ident()
                    .map_or_else(Span::call_site, Ident::span);
                if let Err(err) = bind(local, span, scope) {
                    combine(&mut self.errors, err);
                }
            }
        }
        visit_mut::visit_block_mut(self, block);
    }

    fn visit_macro_mut(&mut self, mac: &mut Macro) {
        if let Some(span) = marker_in(mac.tokens.clone()) {
            let msg = "a `shared` binding cannot be declared inside a macro call; declare it \
                       before the call";
            combine(&mut self.errors, syn::Error::new(span, msg));
        }
    }
}

fn bind(local: &mut Local, span: Span, scope: &mut [Stmt]) -> syn::Result<()> {
    let Some(init) = &mut local.init else {
        let msg = "`shared` needs a value, as in `shared name = value;`";
        return Err(syn::Error::new(span, msg));
    };
    let (name, ty) = match &mut local.pat {
        Pat::Ident(pat) => (pat.ident.clone(), None),
        Pat::Type(typed) => match &*typed.pat {
            Pat::Ident(pat) => (pat.ident.clone(), Some(&mut typed.ty)),
            _ => return Ok(()),
        },
        // The token pass lets only a name, with or without a type, through.
        _ => return Ok(()),
    };
    let uses = access::scan(&name, scope, &LOCK);
    let read_only = uses.accesses && !uses.writes && !uses.handle;
    if !read_only {
        access::rewrite(&name, scope, &LOCK);
    }
    let value = &init.expr;
    *init.expr = if read_only {
        parse_quote_spanned!(span=> ::std::sync::Arc::new(#value))
    } else {
        parse_quote_spanned!(span=> ::rustx::Shared::new(#value))
    };
    if let Some(ty) = ty {
        let inner = &ty;
        **ty = if read_only {
            parse_quote_spanned!(span=> ::std::sync::Arc<#inner>)
        } else {
            parse_quote_spanned!(span=> ::rustx::Shared<#inner>)
        };
    }
    Ok(())
}

fn marker_in(tokens: TokenStream) -> Option<Span> {
    tokens.into_iter().find_map(|tree| match tree {
        TokenTree::Ident(ident) if ident == MARKER => Some(ident.span()),
        TokenTree::Group(group) => marker_in(group.stream()),
        _ => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::{ToTokens, quote};

    fn expand(sugar: TokenStream) -> syn::Result<String> {
        crate::try_expand(sugar).map(|file| file.into_token_stream().to_string())
    }

    fn expands_to(sugar: TokenStream, plain: TokenStream) {
        let plain = syn::parse2::<syn::File>(plain).unwrap();
        assert_eq!(
            expand(sugar).unwrap(),
            plain.into_token_stream().to_string()
        );
    }

    fn messages(sugar: TokenStream) -> Vec<String> {
        let err = expand(sugar).unwrap_err();
        err.into_iter().map(|err| err.to_string()).collect()
    }

    #[test]
    fn shared_statement_becomes_a_marked_let() {
        let out = rewrite(quote! { shared s = 1; #[allow(unused)] shared t: u8 = 2; }).unwrap();
        let want = quote! {
            #[__rustx_shared] let s = 1;
            #[allow(unused)] #[__rustx_shared] let t: u8 = 2;
        };
        assert_eq!(out.to_string(), want.to_string());
    }

    #[test]
    fn a_shared_binding_holds_its_value_in_the_rustx_handle() {
        expands_to(
            quote! { fn f() { shared s = S::default(); shared t: Vec<u8> = Vec::new(); s.lock().n = 1; } },
            quote! { fn f() {
                let s = ::rustx::Shared::new(S::default());
                let t: ::rustx::Shared<Vec<u8> > = ::rustx::Shared::new(Vec::new());
                s.lock().n = 1;
            } },
        );
    }

    #[test]
    fn each_statement_with_one_access_takes_its_own_lock() {
        expands_to(
            quote! { fn f() { shared s = State::default(); s.count += 1; let n = s.count; s.tags.push(n); } },
            quote! { fn f() {
                let s = ::rustx::Shared::new(State::default());
                s.lock().count += 1;
                let n = s.lock().count;
                s.lock().tags.push(n);
            } },
        );
    }

    #[test]
    fn two_accesses_in_one_statement_share_one_guard() {
        expands_to(
            quote! { fn f() -> (u64, usize) { shared m = M::default(); m.a = m.b + 1; (m.a, m.paths.len()) } },
            quote! { fn f() -> (u64, usize) {
                let m = ::rustx::Shared::new(M::default());
                { let __rustx_m = &mut *m.lock(); __rustx_m.a = __rustx_m.b + 1 };
                { let __rustx_m = &mut *m.lock(); (__rustx_m.a, __rustx_m.paths.len()) }
            } },
        );
    }

    #[test]
    fn an_access_whose_lock_outlives_a_nested_block_shares_its_guard_with_it() {
        expands_to(
            quote! { fn f() {
                shared s = S::default();
                for x in s.items.clone() { s.total += x; }
                match s.mode { Mode::A => { s.n = 0; } _ => {} }
                v.extend(s.items.iter().map(|x| x * s.k));
            } },
            quote! { fn f() {
                let s = ::rustx::Shared::new(S::default());
                { let __rustx_s = &mut *s.lock(); for x in __rustx_s.items.clone() { __rustx_s.total += x; } }
                { let __rustx_s = &mut *s.lock(); match __rustx_s.mode { Mode::A => { __rustx_s.n = 0; } _ => {} } }
                { let __rustx_s = &mut *s.lock(); v.extend(__rustx_s.items.iter().map(|x| x * __rustx_s.k)) };
            } },
        );
    }

    #[test]
    fn a_condition_without_let_and_each_body_statement_lock_on_their_own() {
        expands_to(
            quote! { fn f() {
                shared s = S::default();
                while s.n < 3 { s.n += 1; }
                if s.done { return; } else if s.n > 1 { s.n = 0; }
                if let Some(x) = s.next { s.n += x; }
            } },
            quote! { fn f() {
                let s = ::rustx::Shared::new(S::default());
                while s.lock().n < 3 { s.lock().n += 1; }
                if s.lock().done { return; } else if s.lock().n > 1 { s.lock().n = 0; }
                { let __rustx_s = &mut *s.lock(); if let Some(x) = __rustx_s.next { __rustx_s.n += x; } }
            } },
        );
    }

    #[test]
    fn move_closures_and_async_blocks_lock_on_their_own() {
        expands_to(
            quote! { fn f() {
                shared s = S::default();
                s.n = 1;
                let a = s.n + spawn(move || s.n + s.m).join().unwrap();
                let fut = async move { s.n };
            } },
            quote! { fn f() {
                let s = ::rustx::Shared::new(S::default());
                s.lock().n = 1;
                let a = s.lock().n + spawn(move || { let __rustx_s = &mut *s.lock(); __rustx_s.n + __rustx_s.m }).join().unwrap();
                let fut = async move { s.lock().n };
            } },
        );
    }

    #[test]
    fn handle_methods_and_rebinding_clones_stay_on_the_handle() {
        expands_to(
            quote! { fn f() {
                shared s = S::default();
                let n = s.with(|v| v.n);
                s.with_mut(|v| v.n += 1);
                s.lock().n = 2;
                let poisoned = s.is_poisoned();
                let t = move[s.clone()] || s.n;
            } },
            quote! { fn f() {
                let s = ::rustx::Shared::new(S::default());
                let n = s.with(|v| v.n);
                s.with_mut(|v| v.n += 1);
                s.lock().n = 2;
                let poisoned = s.is_poisoned();
                let t = { let s = s.clone(); move || s.lock().n };
            } },
        );
    }

    #[test]
    fn a_pattern_binding_the_name_again_ends_the_scope() {
        expands_to(
            quote! { fn f() {
                shared s = S::default();
                s.n = 1;
                let g = |s: T| s.n;
                match o { Some(s) => s.n, None => s.n }
                for s in v { s.n; }
                if let Some(s) = o { s.n; }
                { let s = T::new(); s.n; }
                s.n;
                let s = T::new();
                s.n;
            } },
            quote! { fn f() {
                let s = ::rustx::Shared::new(S::default());
                s.lock().n = 1;
                let g = |s: T| s.n;
                match o { Some(s) => s.n, None => s.lock().n }
                for s in v { s.n; }
                if let Some(s) = o { s.n; }
                { let s = T::new(); s.n; }
                s.lock().n;
                let s = T::new();
                s.n;
            } },
        );
    }

    #[test]
    fn accesses_inside_expression_macros_are_rewritten() {
        expands_to(
            quote! { fn f() { shared s = S::default(); s.n = 1; println!("{}", s.n); assert_eq!(s.n, s.m); } },
            quote! { fn f() {
                let s = ::rustx::Shared::new(S::default());
                s.lock().n = 1;
                println!("{}", s.lock().n);
                { let __rustx_s = &mut *s.lock(); assert_eq!(__rustx_s.n, __rustx_s.m) };
            } },
        );
    }

    #[test]
    fn a_binding_that_is_only_read_becomes_a_plain_arc() {
        expands_to(
            quote! { fn f(v: Vec<String>) {
                shared paths: Vec<String> = v;
                let n = paths.len();
                let t = move[paths.clone()] || paths.first().cloned();
            } },
            quote! { fn f(v: Vec<String>) {
                let paths: ::std::sync::Arc<Vec<String> > = ::std::sync::Arc::new(v);
                let n = paths.len();
                let t = { let paths = paths.clone(); move || paths.first().cloned() };
            } },
        );
    }

    #[test]
    fn a_write_a_handle_use_or_no_access_keeps_the_lock() {
        for (sugar, plain) in [
            (quote!(v.push(1);), quote!(v.lock().push(1);)),
            (quote!(v.items[0] = 1;), quote!(v.lock().items[0] = 1;)),
            (
                quote!(let r = &mut v.items;),
                quote!(let r = &mut v.lock().items;),
            ),
            (quote!(v.items.sort();), quote!(v.lock().items.sort();)),
            (quote!(v.len(); f(&v);), quote!(v.lock().len(); f(&v);)),
            (
                quote!(v.len(); let w = v.clone();),
                quote!(v.lock().len(); let w = v.clone();),
            ),
            (quote!(), quote!()),
        ] {
            expands_to(
                quote! { fn f() { shared v = V::new(); #sugar } },
                quote! { fn f() { let v = ::rustx::Shared::new(V::new()); #plain } },
            );
        }
    }

    #[test]
    fn shared_as_an_identifier_passes_through() {
        let valid = quote! {
            fn shared(shared: u8) -> u8 {
                let shared = 1;
                shared = 2;
                shared += 1;
                shared.x;
                shared(1);
                shared as u8;
                shared[0];
                shared;
                S { shared: 1 };
                shared
            }
        };
        assert_eq!(expand(valid.clone()).unwrap(), valid.to_string());
        let raw = quote! { fn f() { r#shared x = 1; } };
        assert_eq!(rewrite(raw.clone()).unwrap().to_string(), raw.to_string());
    }

    #[test]
    fn lowering_leaves_valid_rust_without_markers_alone() {
        // The attribute macros run the AST passes on valid Rust, where no marker exists.
        let valid = quote! { fn f() { let s = ::rustx::Shared::new(S::default()); s.lock().n += 1; let t = s; } };
        let mut file: syn::File = syn::parse2(valid.clone()).unwrap();
        lower(&mut file).unwrap();
        assert_eq!(file.into_token_stream().to_string(), valid.to_string());
    }

    #[test]
    fn misuse_is_reported_at_the_sugar() {
        for (sugar, message) in [
            (quote!(shared mut s = 1;), "needs no `mut`"),
            (quote!(shared ref s = 1;), "single name"),
            (quote!(shared _ = 1;), "single name"),
            (quote!(shared (a, b) = t;), "single name"),
            (quote!(shared Some(a) = t;), "single name"),
            (quote!(shared a::B = t;), "single name"),
            (quote!(shared n @ 1..=9 = t;), "single name"),
            (quote!(shared s;), "needs a value"),
            (quote!(shared s: u8;), "needs a value"),
            (
                quote!(let v = vec![{ shared s = 1; s }];),
                "inside a macro call",
            ),
        ] {
            let found = messages(quote! { fn f() { #sugar } });
            assert!(
                found.iter().any(|m| m.contains(message)),
                "{sugar}: {found:?}"
            );
        }
    }

    #[test]
    fn every_misuse_in_one_input_is_reported() {
        let found = messages(quote! { fn f() { shared mut a = 1; shared _ = 2; } });
        assert_eq!(found.len(), 2, "{found:?}");
    }
}
