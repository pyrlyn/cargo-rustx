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
//! The AST pass takes the marker off and makes the value a `::rustx::Shared::new(e)`; a written
//! type is the value's, so `shared s: T = e` annotates the handle as `Shared<T>`. A `shared`
//! binding inside a macro call is an error: the AST pass cannot see into the call to rewrite
//! its scope.

use proc_macro2::{Delimiter, Group, Ident, Punct, Spacing, Span, TokenStream, TokenTree};
use syn::visit_mut::{self, VisitMut};
use syn::{Block, Local, Macro, Pat, Stmt, parse_quote_spanned};

use crate::tokens::map_sequences_reporting;
use crate::tokens::position::starts_statement;
use crate::tokens::punct::{is_alone, is_punct, is_single_colon};
use crate::tokens::syntax::{combine, take_attr};

const MARKER: &str = "__rustx_shared";

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
        for stmt in &mut block.stmts {
            if let Stmt::Local(local) = stmt
                && let Some(marker) = take_attr(&mut local.attrs, MARKER)
            {
                let span = marker
                    .path()
                    .get_ident()
                    .map_or_else(Span::call_site, Ident::span);
                if let Err(err) = bind(local, span) {
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

fn bind(local: &mut Local, span: Span) -> syn::Result<()> {
    let Some(init) = &mut local.init else {
        let msg = "`shared` needs a value, as in `shared name = value;`";
        return Err(syn::Error::new(span, msg));
    };
    let value = &init.expr;
    *init.expr = parse_quote_spanned!(span=> ::rustx::Shared::new(#value));
    // The token pass lets only a name, with or without a type, through.
    if let Pat::Type(typed) = &mut local.pat {
        let inner = &typed.ty;
        *typed.ty = parse_quote_spanned!(span=> ::rustx::Shared<#inner>);
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
