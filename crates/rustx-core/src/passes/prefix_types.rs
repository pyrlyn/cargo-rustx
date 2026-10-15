//! Type-prefix sugar: `~Trait`, `iter T`, `rc T`, `weak T` (problems 04, 09, 16).
//!
//! A token pass: a keyword or `~` before a type is not valid Rust, so it is rewritten before
//! parsing. Expansions use fully qualified paths so user imports neither break nor change them:
//!
//! - `~T` → `::std::boxed::Box<dyn T + ::core::marker::Send + ::core::marker::Sync>`
//! - `iter T` → `impl ::core::iter::Iterator<Item = T> + '_`
//! - `rc T` → `::std::rc::Rc<::core::cell::RefCell<T>>`
//! - `weak T` → `::std::rc::Weak<::core::cell::RefCell<T>>`
//!
//! # Detection
//!
//! `~` is never valid Rust, so every `~` is sugar and must be followed by a trait (a path, a
//! `Fn(..) -> ..` bound, or `(A + B)`); anything else is an error on the `~`.
//!
//! `iter`, `rc` and `weak` are ordinary identifiers in valid Rust, so they are sugar only when
//! both of these hold, otherwise they pass through untouched:
//!
//! 1. They stand in a type position, judged from the preceding tokens: after `->`, after a
//!    single `:`, after `<`, after `,` inside an unclosed `<`, after `=` in `Item = ..` or a
//!    `type` alias, or after `&`, `&mut`, `&'a`, `*const`, `*mut` or another prefix that is
//!    itself in a type position.
//! 2. The next token cannot follow an identifier in valid Rust: an identifier (keywords that
//!    cannot start a type are rejected by the type parser below). After `->` or after
//!    another prefix, `(`, `[`, `&` and `*` also count; elsewhere they could be a call, an index
//!    or an operator (`S { f: rc(x) }`, `a < rc[0]`).
//!
//! A path or generic use of the name (`iter::Empty<T>`, `rc<T>`) is never sugar, and `r#rc`
//! always means the identifier.
//!
//! A prefix directly followed by `~` (`rc ~Shape`) is sugar in any position: `~` is never valid
//! Rust, so the pair cannot be an identifier use.
//!
//! # End of the type
//!
//! The tokens after the prefix are parsed with `syn::Type::without_plus` (the same grammar as
//! after `&`), and the number of token trees it consumed marks the end. Parsing stops at the
//! first `;`, which no type contains outside a group. Prefixes are expanded right to left, so a
//! nested prefix (`rc ~Shape`, `iter rc Node`) is already plain Rust when the outer one parses.
//! A `+` after the type belongs to the surrounding bound list, as with `&T`: write `~(A + B)`.
//!
//! # Limitations
//!
//! - No type positions at the start of a group or after `,` outside `<..>`: tuple types, tuple
//!   struct fields, fn-pointer parameters and array elements need the plain Rust spelling,
//!   because `f(rc x)` there is the expression sugar of problem 16.
//! - A struct literal field (`S { f: rc x }`) looks like a typed field and is rewritten as a type.
//! - Macro bodies are token trees like any other, so the rules above apply inside them too.
//! - `iter T` always adds `+ '_`, which needs an elided lifetime in the signature.

use proc_macro2::{Delimiter, TokenStream, TokenTree};
use quote::quote_spanned;
use syn::parse::{Parse, ParseStream, Parser};
use syn::punctuated::Punctuated;
use syn::{Token, TraitBound, Type, TypeParamBound};

use crate::tokens::map_sequences;
use crate::tokens::punct::{alone, follows_arrow, is_arrow, is_punct, is_single_colon, loosen_end};

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    map_sequences(tokens, &mut rewrite_sequence)
}

#[derive(Clone, Copy)]
enum Prefix {
    Boxed,
    Iter,
    Rc,
    Weak,
}

fn rewrite_sequence(seq: Vec<TokenTree>) -> syn::Result<Vec<TokenTree>> {
    // Detection reads the sequence as the user wrote it; expansion then runs right to left so
    // every index found here stays valid while later ones are spliced.
    let found: Vec<(usize, Prefix)> = (0..seq.len())
        .filter_map(|i| sugar_at(&seq, i).map(|prefix| (i, prefix)))
        .collect();
    let mut out = seq;
    for &(i, prefix) in found.iter().rev() {
        let Some((used, expansion)) = expand(prefix, &out[i], &out[i + 1..])? else {
            continue;
        };
        out.splice(i..=i + used, expansion);
        if let Some(TokenTree::Punct(before)) = i.checked_sub(1).map(|j| &mut out[j]) {
            // A joint `:` before the replaced sugar would glue onto the leading `::`.
            *before = alone(before);
        }
    }
    Ok(out)
}

fn sugar_at(seq: &[TokenTree], i: usize) -> Option<Prefix> {
    match &seq[i] {
        TokenTree::Punct(p) if p.as_char() == '~' => Some(Prefix::Boxed),
        TokenTree::Ident(ident) => {
            let prefix = match ident.to_string().as_str() {
                "iter" => Prefix::Iter,
                "rc" => Prefix::Rc,
                "weak" => Prefix::Weak,
                _ => return None,
            };
            let follows = match seq.get(i + 1)? {
                TokenTree::Ident(_) => true,
                // `rc ~T` is neither valid Rust nor an expression, so it is a type anywhere.
                TokenTree::Punct(p) if p.as_char() == '~' => return Some(prefix),
                TokenTree::Punct(p) if matches!(p.as_char(), '&' | '*') => strong_position(seq, i),
                TokenTree::Group(g) if g.delimiter() != Delimiter::Brace => strong_position(seq, i),
                _ => false,
            };
            (follows && in_type_position(seq, i)).then_some(prefix)
        }
        _ => None,
    }
}

/// Whether the token at `i` starts a type, judged only from the tokens before it.
fn in_type_position(seq: &[TokenTree], i: usize) -> bool {
    let Some(p) = i.checked_sub(1) else {
        return false;
    };
    match &seq[p] {
        TokenTree::Punct(punct) => match punct.as_char() {
            '>' => follows_arrow(seq, i),
            ':' => is_single_colon(seq, p),
            '<' => true,
            ',' => inside_generics(seq, p),
            '=' => binds_type(seq, p),
            '&' | '*' => in_type_position(seq, p),
            _ => false,
        },
        TokenTree::Ident(ident) => {
            let before = p.checked_sub(1).map(|q| &seq[q]);
            if ident == "mut" || ident == "const" {
                // `&mut T`, `*const T`, `*mut T`, `&'a mut T`
                let modifies = match before {
                    Some(TokenTree::Punct(b)) => matches!(b.as_char(), '&' | '*'),
                    Some(TokenTree::Ident(_)) => is_lifetime_name(seq, p - 1),
                    _ => false,
                };
                modifies && in_type_position(seq, p)
            } else if is_lifetime_name(seq, p) {
                // `&'a T`: the position is that of the `'`, which follows the `&`.
                in_type_position(seq, p - 1)
            } else {
                sugar_at(seq, p).is_some()
            }
        }
        _ => false,
    }
}

/// After `->` or another prefix, an identifier cannot be a call, an index or an operand.
fn strong_position(seq: &[TokenTree], i: usize) -> bool {
    i.checked_sub(1).is_some_and(|p| match &seq[p] {
        TokenTree::Punct(_) => follows_arrow(seq, i),
        TokenTree::Ident(_) => sugar_at(seq, p).is_some(),
        _ => false,
    })
}

/// Whether the identifier at `i` is the name part of a lifetime (`'a`), not a plain identifier.
fn is_lifetime_name(seq: &[TokenTree], i: usize) -> bool {
    i.checked_sub(1).is_some_and(|q| is_punct(&seq[q], '\''))
}

/// Whether the token at `i` sits inside an unclosed `<`, scanning back to the statement start.
///
/// Not `tokens::angle`: that counts forward from a known `<`, while this scans backward from an
/// arbitrary token, where `>` opens a level and `<` closes one.
fn inside_generics(seq: &[TokenTree], i: usize) -> bool {
    let mut depth = 0usize;
    for k in (0..i).rev() {
        match &seq[k] {
            TokenTree::Punct(p) if p.as_char() == ';' => return false,
            TokenTree::Punct(p) if p.as_char() == '<' => match depth.checked_sub(1) {
                Some(d) => depth = d,
                None => return true,
            },
            TokenTree::Punct(p) if p.as_char() == '>' && !is_arrow(seq, k) => depth += 1,
            _ => {}
        }
    }
    false
}

/// Whether the `=` at `eq` is followed by a type: `Item = T` in generics, or a `type` alias.
fn binds_type(seq: &[TokenTree], eq: usize) -> bool {
    let Some(name) = eq.checked_sub(1) else {
        return false;
    };
    match &seq[name] {
        TokenTree::Ident(_) => {
            let binding = name.checked_sub(1).is_some_and(|open| {
                is_punct(&seq[open], '<')
                    || (is_punct(&seq[open], ',') && inside_generics(seq, open))
            });
            if binding {
                return true;
            }
        }
        // `type Alias<T> = ..` ends its generics right before the `=`.
        TokenTree::Punct(p) if p.as_char() == '>' => {}
        // `==`, `<=`, `+=` and friends compare or assign values.
        _ => return false,
    }
    seq[..name]
        .iter()
        .rev()
        .take_while(|tree| !is_punct(tree, ';'))
        .any(|tree| matches!(tree, TokenTree::Ident(ident) if ident == "type"))
}

fn expand(
    prefix: Prefix,
    sugar: &TokenTree,
    rest: &[TokenTree],
) -> syn::Result<Option<(usize, TokenStream)>> {
    let span = sugar.span();
    let (inner, used) = match prefix {
        Prefix::Boxed => parse_bounds(rest).ok_or_else(|| not_a_trait(span))?,
        Prefix::Iter | Prefix::Rc | Prefix::Weak => match parse_front(rest, Type::without_plus) {
            Some((ty, used)) => (unparen(&ty, &rest[..used]), used),
            // Not followed by a type after all (`-> rc where ..`): the name is plain Rust.
            None => return Ok(None),
        },
    };
    let tokens = match prefix {
        Prefix::Boxed => quote_spanned!(span=>
            ::std::boxed::Box<dyn #inner + ::core::marker::Send + ::core::marker::Sync>
        ),
        Prefix::Iter => quote_spanned!(span=> impl ::core::iter::Iterator<Item = #inner> + '_),
        Prefix::Rc => quote_spanned!(span=> ::std::rc::Rc<::core::cell::RefCell<#inner>>),
        Prefix::Weak => quote_spanned!(span=> ::std::rc::Weak<::core::cell::RefCell<#inner>>),
    };
    Ok(Some((used, tokens)))
}

/// Parses one `T` from the front of `rest`; returns it and how many token trees it spans.
fn parse_front<T>(
    rest: &[TokenTree],
    parse: fn(ParseStream) -> syn::Result<T>,
) -> Option<(T, usize)> {
    let end = rest
        .iter()
        .position(|tree| is_punct(tree, ';'))
        .unwrap_or(rest.len());
    let parser = |input: ParseStream| {
        let value = parse(input)?;
        let left: TokenStream = input.parse()?;
        Ok((value, left.into_iter().count()))
    };
    let (value, left) = parser.parse2(rest[..end].iter().cloned().collect()).ok()?;
    Some((value, end - left))
}

/// The bounds after `~` to put after `dyn`. A trait is parsed as a bound, not a type, because
/// `Fn(A) -> B` is only a bound.
fn parse_bounds(rest: &[TokenTree]) -> Option<(TokenStream, usize)> {
    if let Some(TokenTree::Group(group)) = rest.first()
        && group.delimiter() == Delimiter::Parenthesis
    {
        // `~(A + B)`: the parentheses only group the bounds; `dyn (A + B)` is not Rust.
        Punctuated::<TypeParamBound, Token![+]>::parse_separated_nonempty
            .parse2(group.stream())
            .ok()?;
        return Some((loosen_end(group.stream()), 1));
    }
    let (_, used) = parse_front(rest, TraitBound::parse)?;
    Some((loosen_end(rest[..used].iter().cloned()), used))
}

/// The written type, without one pair of parentheses around it: `RefCell<(T)>` trips
/// `unused_parens` in the user's crate, since the tokens carry the user's spans.
fn unparen(ty: &Type, written: &[TokenTree]) -> TokenStream {
    match (ty, written) {
        (Type::Paren(_), [TokenTree::Group(group)]) => loosen_end(group.stream()),
        _ => loosen_end(written.iter().cloned()),
    }
}

fn not_a_trait(span: proc_macro2::Span) -> syn::Error {
    syn::Error::new(
        span,
        "expected a trait after `~`, as in `~Trait` or `~Fn(T) -> U`",
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::{ToTokens, quote};

    /// Both sides re-printed through `syn`, so token spacing does not matter.
    fn expands_to(sugar: TokenStream, plain: TokenStream) {
        let normalize = |tokens: TokenStream| {
            syn::parse2::<syn::File>(tokens)
                .unwrap()
                .into_token_stream()
                .to_string()
        };
        assert_eq!(normalize(rewrite(sugar).unwrap()), normalize(plain));
    }

    fn unchanged(valid: &TokenStream) {
        assert_eq!(
            rewrite(valid.clone()).unwrap().to_string(),
            valid.to_string()
        );
    }

    #[test]
    fn tilde_becomes_send_sync_box_dyn() {
        expands_to(
            quote! { fn add_sink(s: ~Sink) {} },
            quote! { fn add_sink(s: ::std::boxed::Box<dyn Sink + ::core::marker::Send + ::core::marker::Sync>) {} },
        );
    }

    #[test]
    fn tilde_keeps_fn_sugar_and_works_inside_generics() {
        expands_to(
            quote! { struct Bus { h: HashMap<String, Vec<~Fn(&Event) -> bool>> } },
            quote! { struct Bus { h: HashMap<String, Vec<::std::boxed::Box<dyn Fn(&Event) -> bool + ::core::marker::Send + ::core::marker::Sync>>> } },
        );
    }

    #[test]
    fn tilde_unwraps_parenthesized_bounds() {
        expands_to(
            quote! { type S = ~(Sink + Debug); },
            quote! { type S = ::std::boxed::Box<dyn Sink + Debug + ::core::marker::Send + ::core::marker::Sync>; },
        );
    }

    #[test]
    fn tilde_glued_to_colon_still_parses() {
        let out = rewrite("fn f(s:~Sink) {}".parse().unwrap()).unwrap();
        syn::parse2::<syn::File>(out).unwrap();
    }

    #[test]
    fn tilde_without_a_trait_is_an_error() {
        let err = rewrite(quote! { fn f(s: Vec<~>) {} }).unwrap_err();
        assert!(
            err.to_string().starts_with("expected a trait after `~`"),
            "{err}"
        );
    }

    #[test]
    fn tilde_before_a_non_trait_type_is_an_error() {
        let err = rewrite(quote! { fn f(s: ~&Sink) {} }).unwrap_err();
        assert!(
            err.to_string().starts_with("expected a trait after `~`"),
            "{err}"
        );
    }

    #[test]
    fn iter_return_becomes_impl_iterator() {
        expands_to(
            quote! { fn evens(v: &[i32]) -> iter &i32 { v.iter() } },
            quote! { fn evens(v: &[i32]) -> impl ::core::iter::Iterator<Item = &i32> + '_ { v.iter() } },
        );
    }

    #[test]
    fn iter_of_a_tuple_after_arrow() {
        expands_to(
            quote! { fn f(v: &[i32]) -> iter (usize, &i32) { v.iter().enumerate() } },
            quote! { fn f(v: &[i32]) -> impl ::core::iter::Iterator<Item = (usize, &i32)> + '_ { v.iter().enumerate() } },
        );
    }

    #[test]
    fn rc_and_weak_become_refcell_pointers() {
        expands_to(
            quote! { struct Node { parent: Option<weak Node>, children: Vec<rc Node> } },
            quote! { struct Node {
                parent: Option<::std::rc::Weak<::core::cell::RefCell<Node>>>,
                children: Vec<::std::rc::Rc<::core::cell::RefCell<Node>>>
            } },
        );
    }

    #[test]
    fn rc_behind_a_reference() {
        expands_to(
            quote! { fn f(a: &rc Node, b: &'a mut rc Node) {} },
            quote! { fn f(a: &::std::rc::Rc<::core::cell::RefCell<Node>>, b: &'a mut ::std::rc::Rc<::core::cell::RefCell<Node>>) {} },
        );
    }

    #[test]
    fn nested_prefixes_expand_inside_out() {
        expands_to(
            quote! { fn f(s: rc ~Shape) -> iter rc Node {} },
            quote! { fn f(s: ::std::rc::Rc<::core::cell::RefCell<::std::boxed::Box<dyn Shape + ::core::marker::Send + ::core::marker::Sync>>>)
            -> impl ::core::iter::Iterator<Item = ::std::rc::Rc<::core::cell::RefCell<Node>>> + '_ {} },
        );
    }

    #[test]
    fn prefix_before_tilde_expands_even_at_group_start() {
        expands_to(
            quote! { fn f(all: &[rc ~Shape]) {} },
            quote! { fn f(all: &[::std::rc::Rc<::core::cell::RefCell<::std::boxed::Box<dyn Shape + ::core::marker::Send + ::core::marker::Sync>>>]) {} },
        );
    }

    #[test]
    fn rc_in_second_generic_argument_and_assoc_binding() {
        expands_to(
            quote! { fn f(m: HashMap<u8, rc Node>, i: impl Iterator<Item = weak Node>) {} },
            quote! { fn f(m: HashMap<u8, ::std::rc::Rc<::core::cell::RefCell<Node>>>,
            i: impl Iterator<Item = ::std::rc::Weak<::core::cell::RefCell<Node>>>) {} },
        );
    }

    #[test]
    fn rc_in_type_alias() {
        expands_to(
            quote! { type Link<T> = rc Vec<T>; },
            quote! { type Link<T> = ::std::rc::Rc<::core::cell::RefCell<Vec<T>>>; },
        );
    }

    #[test]
    fn prefix_names_used_as_identifiers_stay() {
        unchanged(&quote! {
            fn iter(&self) -> Iter<'_, u8> { self.v.iter() }
            fn g(rc: Rc<u8>, iter: I, weak: W) -> u8 {
                let rc = rc.clone();
                let n = S { rc: rc(1), weak: weak[0], iter: iter & mask };
                if a < rc(b) { *rc } else { iter.count() }
            }
        });
    }

    #[test]
    fn prefix_names_as_types_and_paths_stay() {
        unchanged(&quote! {
            fn f<'rc, T: iter>(x: rc, y: iter::Empty<T>, z: rc<T>, w: &'rc weak) -> rc where T: Copy {}
            impl iter for rc {}
            fn g() -> r#rc {}
        });
    }

    #[test]
    fn expression_prefixes_are_left_for_expression_sugar() {
        unchanged(&quote! { fn f() { let a = rc Node {}; g(rc x, weak y); } });
    }
}
