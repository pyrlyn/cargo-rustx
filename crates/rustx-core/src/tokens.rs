//! Token-tree walking shared by every token pass.
//!
//! Separate so passes do not each re-implement group recursion and span bookkeeping.

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

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro2::{Ident, Span};
    use quote::quote;

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
}
