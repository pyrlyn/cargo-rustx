//! `from` variants `Io(from io::Error)` → `From` impls, and `#[display(..)]` → `Display` and
//! `Error` impls (problem 05).
//!
//! # Two halves
//!
//! `Io(from io::Error)` does not parse as Rust, so a token pass (this module) turns it into
//! `#[__rustx_from] Io(io::Error)`; the AST pass ([`lower`]) then generates, after the enum:
//!
//! - `impl From<io::Error> for E` for every marked variant;
//! - when variants carry `#[display("..")]`: `impl Display for E` with one `write!` per variant,
//!   and an empty `impl std::error::Error for E`, as in the problem's "As is" code (no `source()`).
//!
//! The impls copy the enum's generics and `where` clause and add no bounds of their own, and they
//! run before `inline_bounds` and `#[borrows]`, which extend them like hand-written impls.
//!
//! # Detection
//!
//! `from` is sugar only as the first token of a tuple variant's field inside an `enum` body, and
//! only when the next token cannot continue a type named `from`: an identifier, `&`, `*` or
//! `[..]`. So `V(from)`, `V(from::T)`, `V(from<T>)`, `V(from!(..))` and `V(from(T))` stay Rust,
//! and `from (A, B)` or `from ::a::B` are not sugar either. `from` in a named field
//! (`V { e: from T }`) or a variant with several fields is an error. Only the `from T` spelling
//! is accepted: problem 05 does not show `#[from]`.
//!
//! # `#[display]`
//!
//! The string's placeholders name fields: `{0}` and `{}` a tuple field, `{name}` a named field,
//! each with an optional `:spec`. Once one variant has `#[display]`, every variant needs one.
//! An enum that derives `Display` (`derive_more`, `parse-display`) keeps its attributes for that
//! derive, and one that derives `Error` gets no `Error` impl from this pass.

pub(super) mod format;
mod lower;

pub(super) use lower::lower;

use proc_macro2::{Delimiter, Group, Ident, Punct, Spacing, Span, TokenStream, TokenTree};

use crate::tokens::angle::angle_depth;
use crate::tokens::map_sequences_reporting;
use crate::tokens::punct::{is_punct, is_single_colon};
use crate::tokens::syntax::combine;

/// The variant attribute the token half leaves on every `from` variant for [`lower`].
const FROM_MARKER: &str = "__rustx_from";

const FROM_SHAPE: &str =
    "`from` needs a tuple variant with exactly one field, as in `Io(from io::Error)`";

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    map_sequences_reporting(tokens, |seq, errors| {
        let mut out = seq.to_vec();
        for at in enum_bodies(seq) {
            if let TokenTree::Group(body) = &seq[at] {
                out[at] = TokenTree::Group(rewrite_body(body, errors));
            }
        }
        out
    })
}

/// Indices of the `{ .. }` bodies of the enums declared in `seq`.
fn enum_bodies(seq: &[TokenTree]) -> Vec<usize> {
    let mut bodies = Vec::new();
    for at in 0..seq.len() {
        if !is_ident(&seq[at], "enum") || !matches!(seq.get(at + 1), Some(TokenTree::Ident(_))) {
            continue;
        }
        // Generics and the `where` clause come first; a `{ .. }` inside `<..>` is a const argument.
        let mut depth = 0;
        for i in at + 2..seq.len() {
            match &seq[i] {
                TokenTree::Group(g) if depth == 0 && g.delimiter() == Delimiter::Brace => {
                    bodies.push(i);
                    break;
                }
                tree if is_punct(tree, ';') => break,
                _ => depth = angle_depth(seq, i, depth),
            }
        }
    }
    bodies
}

fn rewrite_body(body: &Group, errors: &mut Option<syn::Error>) -> Group {
    let seq: Vec<TokenTree> = body.stream().into_iter().collect();
    // Variant fields sit in groups, so only discriminants could hold a top-level comma: a turbofish
    // cut there leaves pieces that never look like a variant.
    let mut out = Vec::with_capacity(seq.len());
    let mut start = 0;
    for end in 0..=seq.len() {
        if end == seq.len() || is_punct(&seq[end], ',') {
            out.extend(rewrite_variant(&seq[start..end], errors));
            out.extend(seq.get(end).cloned());
            start = end + 1;
        }
    }
    let mut rebuilt = Group::new(Delimiter::Brace, out.into_iter().collect());
    rebuilt.set_span(body.span());
    rebuilt
}

fn rewrite_variant(variant: &[TokenTree], errors: &mut Option<syn::Error>) -> Vec<TokenTree> {
    let name = skip_attributes(variant, 0);
    let (Some(TokenTree::Ident(_)), Some(TokenTree::Group(fields))) =
        (variant.get(name), variant.get(name + 1))
    else {
        return variant.to_vec();
    };
    let inner: Vec<TokenTree> = fields.stream().into_iter().collect();
    let parts = split_fields(&inner);
    match fields.delimiter() {
        Delimiter::Parenthesis => {
            let sugar: Vec<(usize, Span)> = parts
                .iter()
                .filter_map(|&(start, end)| {
                    let at = skip_attributes(&inner[..end], start);
                    from_sugar(&inner[..end], at).map(|span| (at, span))
                })
                .collect();
            match sugar[..] {
                [] => {}
                [(at, span)] if parts.len() == 1 => {
                    let mut stripped = inner;
                    stripped.remove(at);
                    let mut group =
                        Group::new(Delimiter::Parenthesis, stripped.into_iter().collect());
                    group.set_span(fields.span());
                    let mut out = variant[..name].to_vec();
                    out.extend(marker(span));
                    out.extend([variant[name].clone(), TokenTree::Group(group)]);
                    out.extend_from_slice(&variant[name + 2..]);
                    return out;
                }
                _ => sugar
                    .iter()
                    .for_each(|&(_, span)| combine(errors, syn::Error::new(span, FROM_SHAPE))),
            }
        }
        Delimiter::Brace => {
            for &(start, end) in &parts {
                let colon = (start..end).find(|&i| is_single_colon(&inner, i));
                if let Some(span) = colon.and_then(|colon| from_sugar(&inner[..end], colon + 1)) {
                    combine(errors, syn::Error::new(span, FROM_SHAPE));
                }
            }
        }
        _ => {}
    }
    variant.to_vec()
}

/// `(start, end)` of each comma-separated field, without a trailing empty one.
fn split_fields(seq: &[TokenTree]) -> Vec<(usize, usize)> {
    let mut parts = Vec::new();
    let (mut start, mut depth) = (0, 0);
    for i in 0..seq.len() {
        if depth == 0 && is_punct(&seq[i], ',') {
            parts.push((start, i));
            start = i + 1;
        }
        depth = angle_depth(seq, i, depth);
    }
    if start < seq.len() {
        parts.push((start, seq.len()));
    }
    parts
}

/// The span of `from` at `at` when it is sugar rather than a type named `from`.
fn from_sugar(field: &[TokenTree], at: usize) -> Option<Span> {
    if !is_ident(field.get(at)?, "from") {
        return None;
    }
    let starts_type = match field.get(at + 1)? {
        TokenTree::Ident(_) => true,
        TokenTree::Punct(p) => matches!(p.as_char(), '&' | '*'),
        TokenTree::Group(g) => g.delimiter() == Delimiter::Bracket,
        TokenTree::Literal(_) => false,
    };
    starts_type.then(|| field[at].span())
}

/// Index of the first token after the outer attributes (`#[..]`) starting at `at`.
fn skip_attributes(seq: &[TokenTree], mut at: usize) -> usize {
    while seq.get(at).is_some_and(|tree| is_punct(tree, '#'))
        && matches!(seq.get(at + 1), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Bracket)
    {
        at += 2;
    }
    at
}

fn is_ident(tree: &TokenTree, name: &str) -> bool {
    matches!(tree, TokenTree::Ident(ident) if ident == name)
}

/// `#[__rustx_from]`, spanned on the user's `from` so [`lower`] can point its impl there.
fn marker(span: Span) -> [TokenTree; 2] {
    let mut hash = Punct::new('#', Spacing::Alone);
    hash.set_span(span);
    let name = TokenTree::Ident(Ident::new(FROM_MARKER, span));
    let mut attr = Group::new(Delimiter::Bracket, name.into());
    attr.set_span(span);
    [TokenTree::Punct(hash), TokenTree::Group(attr)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    fn compact(tokens: &TokenStream) -> String {
        tokens.to_string().split_whitespace().collect()
    }

    fn expand(tokens: TokenStream) -> String {
        compact(&rewrite(tokens).unwrap())
    }

    fn errors(tokens: TokenStream) -> Vec<String> {
        rewrite(tokens)
            .unwrap_err()
            .into_iter()
            .map(|err| err.to_string())
            .collect()
    }

    #[test]
    fn from_in_a_single_field_variant_becomes_a_marker() {
        assert_eq!(
            expand(quote! {
                pub enum E<T: Into<u8>, const N: usize = { 1 }> where T: Copy {
                    #[display("io")] Io(from io::Error),
                    Raw(from &'static [T; N]) = 3,
                    Missing(String),
                }
            }),
            compact(&quote! {
                pub enum E<T: Into<u8>, const N: usize = { 1 }> where T: Copy {
                    #[display("io")] #[__rustx_from] Io(io::Error),
                    #[__rustx_from] Raw(&'static [T; N]) = 3,
                    Missing(String),
                }
            })
        );
    }

    #[test]
    fn enums_nested_in_functions_and_modules_are_rewritten() {
        assert_eq!(
            expand(quote! { mod m { fn f() { enum E { A(from *const u8) } } } }),
            compact(&quote! { mod m { fn f() { enum E { #[__rustx_from] A(*const u8) } } } })
        );
    }

    #[test]
    fn valid_rust_naming_from_is_unchanged() {
        let valid = quote! {
            enum E {
                A(from), B(from::T), C(from<T>), D(from!(x)), F(from(u8)), G(#[a] from),
                H { from: u8, g: from::T }, I = f::<A, B>(from), J(Vec<A, from>),
            }
            struct S(from T);
            fn g(from: u8) -> u8 { from }
            enum r#from {}
        };
        assert_eq!(expand(valid.clone()), compact(&valid));
    }

    #[test]
    fn from_in_a_multi_field_or_named_variant_is_an_error_reported_together() {
        assert_eq!(
            errors(quote! {
                enum E { Pair(from u8, from u16), Named { e: from io::Error }, Ok(from u32) }
            }),
            [FROM_SHAPE, FROM_SHAPE, FROM_SHAPE]
        );
    }

    #[test]
    fn a_field_with_attributes_is_still_sugar() {
        assert_eq!(
            expand(quote! { enum E { A(#[x] from u8) } }),
            compact(&quote! { enum E { #[__rustx_from] A(#[x] u8) } })
        );
    }
}
