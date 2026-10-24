//! Named arguments: `port: 6432` in a call's argument list becomes the marker
//! `__rustx_named!(port = 6432)`, which `syn` parses and the walk reads back.
//!
//! The token half fires only at the start of an argument of a call group: a `( .. )` right after
//! a name (`f(..)`, `x.m(..)`, `Type::f(..)`) or after another `(..)`/`[..]` (`f(a)(..)`), and
//! never after `fn` (fn-pointer types) or as the parameter list of `fn name(..)`. Inside the
//! group, arguments split at top-level commas, skipping the parameters of a closure argument,
//! so `|a: u8, b: u8| ..` stays as written. A single `:` after the first name of an argument
//! makes it named; `a::b` does not. Struct literals use braces and are never touched.
//!
//! Markers left in token streams the walk does not read as code (attribute arguments, macro
//! bodies that are not expressions, calls in macro bodies that resolve to no defaulted fn) are
//! turned back into `name: value` ([`restore`]), so other macros receive what the user wrote.

use proc_macro2::{Delimiter, Group, Ident, Punct, Spacing, Span, TokenStream, TokenTree};
use quote::ToTokens;
use syn::ext::IdentExt;
use syn::parse::ParseStream;
use syn::token::Paren;
use syn::{Expr, ExprMacro, Macro, MacroDelimiter, Token};

use crate::tokens::params::{argument_end, parameter_list};
use crate::tokens::punct::{is_punct, is_single_colon};
use crate::tokens::syntax::{RESERVED_PREFIX, combine, introduced_span};
use crate::tokens::{map_sequences, map_sequences_reporting};

/// What follows [`RESERVED_PREFIX`] in the marker macro's name.
const MARKER_SUFFIX: &str = "named";

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    map_sequences_reporting(tokens, |seq, errors| {
        let mut out = seq.to_vec();
        for (at, tree) in seq.iter().enumerate() {
            if let TokenTree::Group(group) = tree
                && is_call_group(seq, at)
            {
                let mut rebuilt =
                    Group::new(Delimiter::Parenthesis, arguments(group.stream(), errors));
                rebuilt.set_span(group.span());
                out[at] = TokenTree::Group(rebuilt);
            }
        }
        out
    })
}

fn is_call_group(seq: &[TokenTree], at: usize) -> bool {
    let TokenTree::Group(group) = &seq[at] else {
        return false;
    };
    if group.delimiter() != Delimiter::Parenthesis {
        return false;
    }
    match at.checked_sub(1).map(|before| &seq[before]) {
        Some(TokenTree::Ident(word)) => {
            word != "fn"
                && at
                    .checked_sub(2)
                    .is_none_or(|keyword| parameter_list(seq, keyword) != Some(at))
        }
        Some(TokenTree::Group(before)) => before.delimiter() != Delimiter::Brace,
        _ => false,
    }
}

fn arguments(stream: TokenStream, errors: &mut Option<syn::Error>) -> TokenStream {
    let args: Vec<TokenTree> = stream.into_iter().collect();
    let mut out = Vec::with_capacity(args.len());
    let mut start = 0;
    while start < args.len() {
        let named = matches!(args[start], TokenTree::Ident(_))
            && start + 1 < args.len()
            && is_single_colon(&args, start + 1);
        let value = if named { start + 2 } else { start };
        let end = argument_end(&args, value);
        if named && value < end {
            out.extend(marker(&args[start], &args[start + 1], &args[value..end]));
        } else {
            if named {
                let message = format!("expected a value after `{}:`", args[start]);
                combine(errors, syn::Error::new(args[start + 1].span(), message));
            }
            out.extend(args[start..end].iter().cloned());
        }
        out.extend(args.get(end).cloned());
        start = end + 1;
    }
    out.into_iter().collect()
}

/// `name: value` → `__rustx_named!(name = value)`; the `=` keeps the span of the `:`.
fn marker(name: &TokenTree, colon: &TokenTree, value: &[TokenTree]) -> Vec<TokenTree> {
    let span = name.span();
    let mut eq = Punct::new('=', Spacing::Alone);
    eq.set_span(colon.span());
    let body = [name.clone(), TokenTree::Punct(eq)]
        .into_iter()
        .chain(value.iter().cloned())
        .collect();
    let mut group = Group::new(Delimiter::Parenthesis, body);
    group.set_span(span);
    let mut bang = Punct::new('!', Spacing::Alone);
    bang.set_span(span);
    let path = Ident::new(
        &format!("{RESERVED_PREFIX}{MARKER_SUFFIX}"),
        introduced_span(span),
    );
    vec![
        TokenTree::Ident(path),
        TokenTree::Punct(bang),
        TokenTree::Group(group),
    ]
}

fn is_marker_name(ident: &Ident) -> bool {
    ident.to_string().strip_prefix(RESERVED_PREFIX) == Some(MARKER_SUFFIX)
}

/// Whether `mac` is a marker; its span is the argument's name.
pub(super) fn marker_name(mac: &Macro) -> Option<Span> {
    let first = mac.tokens.clone().into_iter().next()?;
    mac.path
        .get_ident()
        .is_some_and(is_marker_name)
        .then(|| first.span())
}

/// The name a marker carries, the `=` that stood for its `:`, and the marker itself, to put it
/// back unchanged when the call it is in is not sugar.
pub(super) struct Named {
    pub(super) name: Ident,
    eq: Token![=],
    path: syn::Path,
}

/// `Some` when `expr` is a marker: its name and its value, or the error parsing them.
pub(super) fn parse(expr: &Expr) -> Option<syn::Result<(Named, Expr)>> {
    let Expr::Macro(ExprMacro { mac, .. }) = expr else {
        return None;
    };
    marker_name(mac)?;
    Some(mac.parse_body_with(|input: ParseStream| {
        let named = Named {
            name: Ident::parse_any(input)?,
            eq: input.parse()?,
            path: mac.path.clone(),
        };
        Ok((named, input.parse()?))
    }))
}

/// The marker again, around a value the walk may have rewritten.
pub(super) fn rebuild(named: Named, value: &Expr) -> Expr {
    let span = named.name.span();
    let Named { name, eq, path } = named;
    let mut tokens = name.to_token_stream();
    eq.to_tokens(&mut tokens);
    value.to_tokens(&mut tokens);
    Expr::Macro(ExprMacro {
        attrs: Vec::new(),
        mac: Macro {
            path,
            bang_token: Token![!](span),
            delimiter: MacroDelimiter::Paren(Paren(span)),
            tokens,
        },
    })
}

/// `tokens` with every marker turned back into `name: value`, or `None` when there is none.
pub(super) fn restore(tokens: &TokenStream) -> Option<TokenStream> {
    let mut found = false;
    let out = map_sequences(tokens.clone(), &mut |seq: Vec<TokenTree>| {
        let mut out = Vec::with_capacity(seq.len());
        let mut at = 0;
        while at < seq.len() {
            if let [TokenTree::Ident(path), bang, TokenTree::Group(body), ..] = &seq[at..]
                && is_marker_name(path)
                && is_punct(bang, '!')
                && body.delimiter() == Delimiter::Parenthesis
            {
                found = true;
                out.extend(body.stream().into_iter().enumerate().map(|(i, tree)| {
                    if i == 1 {
                        let mut colon = Punct::new(':', Spacing::Alone);
                        colon.set_span(tree.span());
                        TokenTree::Punct(colon)
                    } else {
                        tree
                    }
                }));
                at += 3;
            } else {
                out.push(seq[at].clone());
                at += 1;
            }
        }
        Ok(out)
    })
    .ok()?;
    found.then_some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compact(tokens: &TokenStream) -> String {
        tokens.to_string().split_whitespace().collect()
    }

    fn marks(sugar: &str) -> String {
        rewrite(sugar.parse().unwrap_or_default())
            .map_or_else(|err| err.to_string(), |out| compact(&out))
    }

    fn same(source: &str) -> String {
        compact(&source.parse().unwrap_or_default())
    }

    #[test]
    fn a_named_argument_becomes_a_marker_in_every_kind_of_call() {
        assert_eq!(
            marks("f(a, port: 1 + 2, t: g(x: 3)); s.m(n: 1); T::new(k: v)(z: 0)"),
            same(
                "f(a, __rustx_named!(port = 1 + 2), __rustx_named!(t = g(__rustx_named!(x = 3)))); \
                 s.m(__rustx_named!(n = 1)); \
                 T::new(__rustx_named!(k = v))(__rustx_named!(z = 0))"
            )
        );
    }

    #[test]
    fn closure_parameters_paths_and_labels_are_not_named_arguments() {
        let plain = "f(|a: u8, b: u8| a + b, move |c: u8| c, || 1, a::b, 'x: loop {}, S { a: 1 })";
        assert_eq!(marks(plain), same(plain));
        assert_eq!(
            marks("f(cb: |a: u8, b: u8| a, n: 1)"),
            same("f(__rustx_named!(cb = |a: u8, b: u8| a), __rustx_named!(n = 1))")
        );
    }

    #[test]
    fn signatures_fn_pointers_and_macro_groups_are_not_calls() {
        let plain = "fn f(a: u8) {} fn g<T>(b: T) {} type F = fn(x: u8); \
                     impl S { fn m(&self, c: u8) {} } m!(a: 1); struct T(pub a::B);";
        assert_eq!(marks(plain), same(plain));
    }

    #[test]
    fn a_name_without_a_value_is_an_error() {
        assert_eq!(marks("f(port:)"), "expected a value after `port:`");
    }

    #[test]
    fn restore_turns_markers_back_into_the_written_arguments() {
        let written: TokenStream = "m!(a(port: 1, t: b(x: 2)), [c(y: 3)])"
            .parse()
            .unwrap_or_default();
        let marked = rewrite(written.clone()).unwrap_or_default();
        assert_eq!(
            restore(&marked).map(|out| compact(&out)),
            Some(compact(&written))
        );
        assert!(restore(&written).is_none());
    }

    #[test]
    fn a_marker_parses_back_into_its_name_and_value() {
        let marked = rewrite("__f(port: 1 + 2)".parse().unwrap_or_default()).unwrap_or_default();
        let call: syn::ExprCall = syn::parse2(marked).unwrap();
        let (named, value) = parse(&call.args[0]).unwrap().unwrap();
        assert_eq!(named.name, "port");
        assert_eq!(value.to_token_stream().to_string(), "1 + 2");
        let again = rebuild(named, &value);
        assert_eq!(
            compact(&again.to_token_stream()),
            same("__rustx_named!(port = 1 + 2)")
        );
    }
}
