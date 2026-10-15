//! Token-tree helpers shared by every pass: the walker here, the rest in submodules.
//!
//! Separate so passes do not each re-implement group recursion, punctuation checks, position
//! detection, angle-bracket matching or error bookkeeping.

pub(crate) mod angle;
pub(crate) mod position;
pub(crate) mod punct;
pub(crate) mod syntax;

use proc_macro2::{Group, TokenStream, TokenTree};

/// Applies `rewrite` to every flat token sequence: the top level and the inside of each group.
///
/// Groups are rewritten innermost first, so `rewrite` sees already-rewritten child groups.
/// Each rebuilt group keeps its delimiter and original span, so errors still point at user code.
pub(crate) fn map_sequences<F>(tokens: TokenStream, rewrite: &mut F) -> syn::Result<TokenStream>
where
    F: FnMut(Vec<TokenTree>) -> syn::Result<Vec<TokenTree>>,
{
    let mut flat = Vec::new();
    for tree in tokens {
        flat.push(match tree {
            TokenTree::Group(group) => {
                let inner = map_sequences(group.stream(), rewrite)?;
                let mut rebuilt = Group::new(group.delimiter(), inner);
                rebuilt.set_span(group.span());
                TokenTree::Group(rebuilt)
            }
            other => other,
        });
    }
    Ok(rewrite(flat)?.into_iter().collect())
}

/// Like [`map_sequences`], but `rewrite` records errors instead of returning them.
///
/// Errors are collected rather than returned from the closure so one run reports every misuse,
/// not just the first one `map_sequences` would stop at.
pub(crate) fn map_sequences_reporting<F>(
    tokens: TokenStream,
    mut rewrite: F,
) -> syn::Result<TokenStream>
where
    F: FnMut(&[TokenTree], &mut Option<syn::Error>) -> Vec<TokenTree>,
{
    let mut errors: Option<syn::Error> = None;
    let out = map_sequences(tokens, &mut |seq| Ok(rewrite(&seq, &mut errors)))?;
    errors.map_or(Ok(out), Err)
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro2::{Ident, Span};
    use quote::quote;

    /// The top-level token trees of `source`, for tests that index into a flat sequence.
    pub(super) fn trees(source: &str) -> Vec<TokenTree> {
        source.parse::<TokenStream>().unwrap().into_iter().collect()
    }

    #[test]
    fn rewrite_reaches_nested_groups() {
        let mut rename_foo = |seq: Vec<TokenTree>| {
            Ok(seq
                .into_iter()
                .map(|tree| match tree {
                    TokenTree::Ident(ident) if ident == "foo" => {
                        TokenTree::Ident(Ident::new("bar", ident.span()))
                    }
                    other => other,
                })
                .collect())
        };
        let out = map_sequences(quote! { foo { (foo [foo]) } }, &mut rename_foo).unwrap();
        assert_eq!(out.to_string(), quote! { bar { (bar [bar]) } }.to_string());
    }

    #[test]
    fn errors_from_rewrite_propagate() {
        let mut fail = |_: Vec<TokenTree>| Err(syn::Error::new(Span::call_site(), "nope"));
        let err = map_sequences(quote! { a (b) }, &mut fail).unwrap_err();
        assert_eq!(err.to_string(), "nope");
    }

    #[test]
    fn reporting_walk_collects_an_error_from_every_sequence() {
        let err = map_sequences_reporting(quote! { a (b [c]) }, |seq, errors| {
            syntax::combine(errors, syn::Error::new(Span::call_site(), "seen"));
            seq.to_vec()
        })
        .unwrap_err();
        assert_eq!(err.into_iter().count(), 3);
    }

    #[test]
    fn reporting_walk_without_errors_returns_the_rewrite() {
        let out = map_sequences_reporting(quote! { a (b) }, |seq, _| seq.to_vec()).unwrap();
        assert_eq!(out.to_string(), quote! { a (b) }.to_string());
    }
}
