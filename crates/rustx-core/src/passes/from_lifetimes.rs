//! Return lifetimes `-> &from(xs) T`, including `iter T` with `from` (problems 01, 09).
//!
//! `&from(a) T` in a return type borrows from parameter `a`: the function gets a fresh lifetime
//! `'__rustx_from_a`, the return reference and every elided lifetime of `a` use it.
//! `&from(a, b) T` ties several parameters to one lifetime, and parameters named together in
//! any two `from(..)` of one signature share it too.
//!
//! `&from(Self) T` and `&from(Type) T` name the lifetime `#[borrows]` gave a type (problem 19)
//! instead of a parameter, in parameters as well as in the return type.
//!
//! # Two halves
//!
//! `&from(a) T` does not parse as Rust, and the parameters it names are only known with the
//! whole signature, so the work is split:
//!
//! 1. A token pass (this module) replaces `from(a, b)` after `&` or `&mut` with a marker
//!    lifetime `'__rustx_mark_…` that encodes the names, so `&from(a) T` becomes `&'mark T`.
//!    It runs before `prefix_types`, so `iter &from(a) T` reaches it as `iter &'mark T`.
//! 2. An AST pass ([`lower`]) resolves the markers of each signature's return type, adds the
//!    lifetimes to the fn generics and rewrites the named parameters.
//!
//! # Detection
//!
//! `&from(..)` is a valid expression (a borrow of a call to a function named `from`), so it is
//! sugar only when the next token starts a type and could not continue an expression: an
//! identifier that can start a type (keywords such as `as`, `else`, `if` cannot). Right after
//! `->`, where no expression can stand, `(`, `[`, `&` and `*` count too. `r#from` is always
//! the identifier.

mod lower;

pub(super) use lower::lower;

use proc_macro2::{Delimiter, Ident, Punct, Spacing, TokenStream, TokenTree};
use quote::ToTokens;
use syn::Token;
use syn::ext::IdentExt;
use syn::parse::{ParseStream, Parser};
use syn::punctuated::Punctuated;

use crate::tokens::map_sequences;
use crate::tokens::punct::{follows_arrow, is_punct};

/// Prefix of the marker lifetimes the token pass leaves for [`lower`].
const MARK: &str = "__rustx_mark_";

/// Keywords that can start a type; every other keyword after `&from(..)` is plain Rust.
const TYPE_KEYWORDS: &[&str] = &[
    "dyn", "impl", "fn", "unsafe", "extern", "for", "Self", "crate", "super",
];

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    map_sequences(tokens, &mut |seq| rewrite_sequence(&seq))
}

fn rewrite_sequence(seq: &[TokenTree]) -> syn::Result<Vec<TokenTree>> {
    let mut out = Vec::with_capacity(seq.len());
    let mut i = 0;
    while i < seq.len() {
        let Some(from) = sugar_at(seq, i) else {
            out.push(seq[i].clone());
            i += 1;
            continue;
        };
        // `&` stays; the marker goes between it and an optional `mut`, as in `&'a mut T`.
        out.push(seq[i].clone());
        out.extend(marker(&seq[from], &seq[from + 1])?);
        if from > i + 1 {
            out.push(seq[i + 1].clone());
        }
        i = from + 2;
    }
    Ok(out)
}

/// Index of `from` when `&from(..)` or `&mut from(..)` at `amp` is sugar.
fn sugar_at(seq: &[TokenTree], amp: usize) -> Option<usize> {
    if !is_punct(&seq[amp], '&') {
        return None;
    }
    let from = match seq.get(amp + 1)? {
        TokenTree::Ident(ident) if ident == "mut" => amp + 2,
        _ => amp + 1,
    };
    match (seq.get(from)?, seq.get(from + 1)?) {
        (TokenTree::Ident(ident), TokenTree::Group(group))
            if ident == "from" && group.delimiter() == Delimiter::Parenthesis => {}
        _ => return None,
    }
    let starts_type = match seq.get(from + 2)? {
        TokenTree::Ident(ident) => can_start_type(ident),
        TokenTree::Group(group) => {
            matches!(
                group.delimiter(),
                Delimiter::Parenthesis | Delimiter::Bracket
            ) && follows_arrow(seq, amp)
        }
        TokenTree::Punct(p) => matches!(p.as_char(), '&' | '*') && follows_arrow(seq, amp),
        TokenTree::Literal(_) => false,
    };
    starts_type.then_some(from)
}

fn can_start_type(ident: &Ident) -> bool {
    TYPE_KEYWORDS.iter().any(|keyword| ident == keyword)
        || syn::parse2::<Ident>(ident.to_token_stream()).is_ok()
}

/// The marker lifetime for `from` and its `(..)` group, spanned on the user's tokens.
fn marker(from: &TokenTree, group: &TokenTree) -> syn::Result<[TokenTree; 2]> {
    let TokenTree::Group(group) = group else {
        return Err(syn::Error::new(group.span(), FROM_SYNTAX));
    };
    // `parse_any` because `self` is a keyword and `from(self)` names the receiver.
    let parser = |input: ParseStream| {
        Punctuated::<Ident, Token![,]>::parse_terminated_with(input, Ident::parse_any)
    };
    let names = parser
        .parse2(group.stream())
        .ok()
        .filter(|names| !names.is_empty())
        .ok_or_else(|| syn::Error::new(group.span(), FROM_SYNTAX))?;
    let names: Vec<Ident> = names.into_iter().collect();
    // `'` on `from` and the name on `(..)`: rustc glues the two into one lifetime token over
    // `from(..)`, so an error spanned on the marker underlines exactly what the user wrote.
    let mut apostrophe = Punct::new('\'', Spacing::Joint);
    apostrophe.set_span(from.span());
    Ok([
        TokenTree::Punct(apostrophe),
        TokenTree::Ident(Ident::new(&encode(&names), group.span())),
    ])
}

const FROM_SYNTAX: &str =
    "expected parameter names in `from(..)`, as in `&from(xs) T` or `&from(a, b) T`";

/// `__rustx_mark_` followed by each name as its length and text: names are identifiers, which
/// never start with a digit, so the encoding is unambiguous and itself an identifier.
fn encode(names: &[Ident]) -> String {
    let mut out = String::from(MARK);
    for name in names {
        let name = name.unraw().to_string();
        out.push_str(&name.chars().count().to_string());
        out.push_str(&name);
    }
    out
}

/// The parameter names a marker lifetime ident encodes, or `None` for any other lifetime.
fn decode(ident: &Ident) -> Option<Vec<String>> {
    let text = ident.to_string();
    let mut rest = text.strip_prefix(MARK)?;
    let mut names = Vec::new();
    while !rest.is_empty() {
        let digits = rest.find(|c: char| !c.is_ascii_digit())?;
        let len: usize = rest[..digits].parse().ok()?;
        let name: String = rest[digits..].chars().take(len).collect();
        if name.chars().count() != len {
            return None;
        }
        rest = &rest[digits + name.len()..];
        names.push(name);
    }
    (!names.is_empty()).then_some(names)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro2::Span;
    use quote::quote;

    fn compact(tokens: &TokenStream) -> String {
        tokens.to_string().split_whitespace().collect()
    }

    fn expand(tokens: TokenStream) -> String {
        compact(&rewrite(tokens).unwrap())
    }

    fn unchanged(valid: &TokenStream) {
        assert_eq!(expand(valid.clone()), compact(valid));
    }

    #[test]
    fn from_reference_becomes_a_marker_lifetime() {
        assert_eq!(
            expand(quote! { fn f(xs: &[u8]) -> &from(xs) u8 {} }),
            compact(&quote! { fn f(xs: &[u8]) -> &'__rustx_mark_2xs u8 {} })
        );
    }

    #[test]
    fn several_names_and_mut_are_encoded() {
        assert_eq!(
            expand(quote! { fn f() -> Option<&mut from(self, r#type) str> {} }),
            compact(&quote! { fn f() -> Option<&'__rustx_mark_4self4type mut str> {} })
        );
    }

    #[test]
    fn from_inside_a_tuple_and_after_iter_is_rewritten() {
        assert_eq!(
            expand(quote! { fn f() -> iter (usize, &from(it) Entry) {} }),
            compact(&quote! { fn f() -> iter (usize, &'__rustx_mark_2it Entry) {} })
        );
    }

    #[test]
    fn slices_count_as_types_only_after_an_arrow() {
        assert_eq!(
            expand(quote! { fn f(v: &[u8]) -> &from(v) [u8] {} }),
            compact(&quote! { fn f(v: &[u8]) -> &'__rustx_mark_1v [u8] {} })
        );
        unchanged(&quote! { let a = &from(v) [0]; });
    }

    #[test]
    fn encoding_round_trips_through_decode() {
        let names = [
            Ident::new("self", Span::call_site()),
            Ident::new("a1_", Span::call_site()),
            Ident::new("été", Span::call_site()),
        ];
        let encoded = Ident::new(&encode(&names), Span::call_site());
        assert_eq!(decode(&encoded).unwrap(), ["self", "a1_", "été"]);
        assert_eq!(decode(&Ident::new("a", Span::call_site())), None);
        assert_eq!(
            decode(&Ident::new("__rustx_mark_9a", Span::call_site())),
            None
        );
    }

    #[test]
    fn valid_rust_using_from_is_unchanged() {
        unchanged(&quote! {
            fn from(x: &u8) -> &u8 { x }
            fn g(x: u8) -> u16 {
                let a = &from(x);
                let b = &from(x) as &u8;
                let c = u16::from(x) + T::from(x) + From::from(x);
                let Some(d) = &from(x) else { return 0 };
                match v { &from(y) if y > 0 => 1, _ => 2 }
                for z in &from(x) {}
                &r#from(x) u8
            }
        });
    }

    #[test]
    fn non_names_inside_from_are_an_error() {
        for bad in [
            quote! { fn f() -> &from() u8 {} },
            quote! { fn f() -> &from(x.y) u8 {} },
        ] {
            let err = rewrite(bad).unwrap_err();
            assert!(
                err.to_string().starts_with("expected parameter names"),
                "{err}"
            );
        }
    }
}
