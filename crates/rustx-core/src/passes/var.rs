//! Statement sugar `var x = ..` → `let mut x = ..` (problem 02).
//!
//! A token pass: `var x` is not a Rust statement, so it must be rewritten before `syn` parses.
//!
//! `var` is rewritten only where it starts a statement and a binding follows it, because `var`
//! is an ordinary identifier in Rust and every other use of it must pass through unchanged:
//!
//! - Statement start: the start of a token sequence (file, block, closure or match-arm body),
//!   after `;`, after a `{ .. }` block, or after an outer or inner attribute.
//! - Binding: an identifier other than `as` follows (`var as u8` is a cast). `var = 1`,
//!   `var += 1`, `var.f()`, `var(1)`, `var[0] = 1`, `var;` and `r#var x` are left alone.
//!
//! Patterns: `var` means exactly `let mut`, so it accepts what `let mut` accepts: one name,
//! optionally `name @ subpattern`, then `: T`, `= e` and `else { .. }` as usual. Destructuring
//! (`var (a, b) = ..`, `var Point { x, y } = ..`, `var Some(x) = ..`) is rejected rather than
//! spread over the bindings, because telling a binding from a constant or unit variant in a
//! pattern (`None`, `MAX`) needs name resolution a macro does not have. `var (a, b);` with no
//! `=` or `:` stays a call of a function named `var`, and `var [a, b] = ..` stays an index
//! assignment, since both are valid Rust. `var mut x`, `var ref x` and `var _` are rejected.
//!
//! Sugar is expanded inside macro-call arguments as well (`vec![..]`, `assert_eq!(..)`), since
//! users expect it there. Limitation: a user macro that itself takes `var <name>` tokens
//! receives `let mut <name>`.

use proc_macro2::{Delimiter, Ident, Span, TokenStream, TokenTree};

use crate::tokens::map_sequences_reporting;
use crate::tokens::position::starts_statement;
use crate::tokens::punct::{is_alone, is_joint};
use crate::tokens::syntax::combine;

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    map_sequences_reporting(tokens, rewrite_sequence)
}

/// What a `var` at statement start turns out to be, judged by the tokens after it.
enum Use {
    Binding,
    Misuse(syn::Error),
    Identifier,
}

fn rewrite_sequence(seq: &[TokenTree], errors: &mut Option<syn::Error>) -> Vec<TokenTree> {
    let mut out = Vec::with_capacity(seq.len() + 1);
    for (i, tree) in seq.iter().enumerate() {
        if let TokenTree::Ident(var) = tree
            && var == "var"
            && starts_statement(seq, i)
        {
            match classify(var.span(), &seq[i + 1..]) {
                Use::Binding => {
                    out.push(TokenTree::Ident(Ident::new("let", var.span())));
                    out.push(TokenTree::Ident(Ident::new("mut", var.span())));
                    continue;
                }
                Use::Misuse(err) => combine(errors, err),
                Use::Identifier => {}
            }
        }
        out.push(tree.clone());
    }
    out
}

fn classify(var: Span, after: &[TokenTree]) -> Use {
    match after {
        [TokenTree::Ident(name), rest @ ..] => {
            let message = match name.to_string().as_str() {
                "as" => return Use::Identifier,
                "mut" => "`var` already makes the binding mutable; remove `mut`",
                "ref" => "`var` binds by value; write `let ref mut x` for a mutable reference",
                "_" => "`var _` binds no name to mutate; write `let _`",
                _ if destructures(rest) => return destructuring(var),
                _ => return Use::Binding,
            };
            Use::Misuse(syn::Error::new(name.span(), message))
        }
        [TokenTree::Group(g), next, ..]
            if g.delimiter() == Delimiter::Parenthesis
                && (is_alone(next, '=') || is_alone(next, ':')) =>
        {
            destructuring(var)
        }
        _ => Use::Identifier,
    }
}

/// Whether the tokens after the first identifier of a pattern make it a path or struct pattern.
fn destructures(rest: &[TokenTree]) -> bool {
    match rest.first() {
        Some(TokenTree::Group(g)) => {
            matches!(g.delimiter(), Delimiter::Parenthesis | Delimiter::Brace)
        }
        Some(tree) => is_joint(tree, ':'),
        None => false,
    }
}

fn destructuring(var: Span) -> Use {
    Use::Misuse(syn::Error::new(
        var,
        "`var` takes a single name; to destructure, write `let` and mark each binding \
         (`let (mut a, mut b) = ..`)",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    fn expand(input: TokenStream) -> syn::Result<String> {
        rewrite(input).map(|out| out.to_string())
    }

    fn assert_unchanged(input: &TokenStream) {
        assert_eq!(expand(input.clone()).unwrap(), input.to_string());
    }

    #[test]
    fn var_statement_becomes_let_mut() {
        let out = expand(quote! { var x = 1; var y: u8 = 2; var z; }).unwrap();
        let want = quote! { let mut x = 1; let mut y: u8 = 2; let mut z; };
        assert_eq!(out, want.to_string());
    }

    #[test]
    fn var_after_block_attribute_and_inside_nested_bodies_is_rewritten() {
        let out = expand(quote! {
            fn f() {
                #![allow(unused)]
                var a = 1;
                if c {} var b = 2;
                #[allow(unused)] var c = 3;
                let g = || { var d = 4; };
                match o { _ => { var e = 5; } }
            }
        })
        .unwrap();
        let want = quote! {
            fn f() {
                #![allow(unused)]
                let mut a = 1;
                if c {} let mut b = 2;
                #[allow(unused)] let mut c = 3;
                let g = || { let mut d = 4; };
                match o { _ => { let mut e = 5; } }
            }
        };
        assert_eq!(out, want.to_string());
    }

    #[test]
    fn var_binding_keeps_subpattern_and_let_else() {
        let out = expand(quote! { var n @ 1..=9 = d else { return; }; }).unwrap();
        assert_eq!(
            out,
            quote! { let mut n @ 1..=9 = d else { return; }; }.to_string()
        );
    }

    #[test]
    fn var_as_an_identifier_passes_through() {
        assert_unchanged(&quote! {
            struct S { var: i32 }
            fn var(var: i32) -> i32 { var }
            fn g(s: S, v: &mut [i32]) {
                let var = 1;
                var + 1;
                var = 2;
                var += 1;
                var == 2;
                var.f();
                var(1);
                var(a, b);
                var[0] = 1;
                var as u8;
                var;
                s.var;
                S { var: 1 };
                S { var };
                let r#var = 0;
                for var in v {}
                match var { var => var }
            }
        });
    }

    #[test]
    fn var_mid_expression_is_not_a_statement() {
        assert_unchanged(&quote! { let a = var x; f(b, var y); });
    }

    #[test]
    fn var_mut_is_rejected() {
        let err = expand(quote! { var mut x = 1; }).unwrap_err();
        assert!(err.to_string().contains("remove `mut`"), "{err}");
    }

    #[test]
    fn var_ref_and_wildcard_are_rejected() {
        let err = expand(quote! { var ref x = 1; }).unwrap_err();
        assert!(err.to_string().contains("by value"), "{err}");
        let err = expand(quote! { var _ = 1; }).unwrap_err();
        assert!(err.to_string().contains("binds no name"), "{err}");
    }

    #[test]
    fn destructuring_patterns_are_rejected() {
        for input in [
            quote! { var (a, b) = t; },
            quote! { var (a, b): (u8, u8) = t; },
            quote! { var Point { x, y } = p; },
            quote! { var Some(x) = o else { return }; },
            quote! { var a::B(x) = o; },
        ] {
            let err = expand(input.clone()).unwrap_err();
            assert!(err.to_string().contains("single name"), "{input}: {err}");
        }
    }

    #[test]
    fn every_misuse_in_one_input_is_reported() {
        let err = expand(quote! { var mut a = 1; fn f() { var _ = 2; } }).unwrap_err();
        assert_eq!(err.into_iter().count(), 2);
    }
}
