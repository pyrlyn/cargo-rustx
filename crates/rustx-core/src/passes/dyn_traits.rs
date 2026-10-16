//! `#[dyn]` traits: `async fn` in a trait that is also used as a `dyn` type (problem 14).
//!
//! `async fn` in a trait is valid Rust and stays native. A trait marked `#[dyn]` or
//! `#[dyn(DynName)]` also gets the attribute of the maintained `dynosaur` crate, which generates
//! the type `DynName` (by default `Dyn` and the trait's name) to use where `dyn Trait` would be,
//! boxing each returned future:
//!
//! ```text
//! #[dyn] pub trait Store { async fn get(&self) -> u8; }
//! #[::rustx::dynosaur::dynosaur(pub DynStore = dyn(box) Store)]
//! pub trait Store { async fn get(&self) -> u8; }
//! ```
//!
//! Two halves, because `dyn` is a keyword and `syn` does not parse `#[dyn]`:
//!
//! 1. Token pass ([`rewrite`]): `#[dyn]` / `#[dyn(Name)]` becomes the marker `#[__rustx_dyn]` /
//!    `#[__rustx_dyn(Name)]`, after checking that it sits once on a trait.
//! 2. AST pass ([`lower`]): the marker on each trait becomes the `dynosaur` attribute, in the
//!    marker's place, so an attribute macro written before it (`trait_variant::make`) still runs
//!    first.
//!
//! `DynName` takes the trait's visibility. Values become `DynName` through its constructors
//! (`DynStore::from_ref(&store)`, `DynStore::new_box(store)`), not by coercion. The generated
//! tokens take the span of `dyn`. `dynosaur`'s expansion names no crate path of its own, so the
//! attribute is reached through the `rustx` re-export and user crates need no dependency on it.
//!
//! `dynosaur` is an optional dependency of `rustx`, behind its `dyn` feature. Without it,
//! [`reject`] reports each `#[dyn]` naming the feature, instead of letting rustc report the
//! unresolved `::rustx::dynosaur` path.
//!
//! # Limitations
//!
//! - Only inside `sugar!`: rustc rejects `#[dyn]` before an attribute macro sees the item.
//! - The path is `::rustx::dynosaur`, so the `rustx` dependency must keep its name.
//! - `dyn Trait` and `~Trait` are not rewritten to `DynName`; the generated name is written out.

use proc_macro2::{Delimiter, Group, Ident, Span, TokenStream, TokenTree};
use quote::{format_ident, quote};
use syn::spanned::Spanned;
use syn::visit_mut::{self, VisitMut};
use syn::{Attribute, ItemTrait, Meta};

use crate::tokens::map_sequences_reporting;
use crate::tokens::position::outer_attributes_start;
use crate::tokens::punct::is_punct;
use crate::tokens::syntax::combine;

/// Name of the attribute that carries `#[dyn]` from the token half to the AST half.
const MARKER: &str = "__rustx_dyn";

const NEEDS_FEATURE: &str = "`#[dyn]` needs the `dyn` feature of `rustx`: `rustx = { version = \"..\", features = [\"dyn\"] }`";

const SHAPE: &str = "expected `#[dyn]` or `#[dyn(DynName)]`, where `DynName` names the dyn type";

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    map_sequences_reporting(tokens, |seq, errors| {
        let mut out = seq.to_vec();
        for i in 0..seq.len() {
            let Some((keyword, group)) = dyn_attr(seq, i) else {
                continue;
            };
            match on_one_trait(seq, i, keyword.span()).and_then(|()| marker(&keyword, &group)) {
                Ok(inner) => {
                    let mut rebuilt = Group::new(Delimiter::Bracket, inner);
                    rebuilt.set_span(group.span());
                    out[i + 1] = TokenTree::Group(rebuilt);
                }
                Err(err) => combine(errors, err),
            }
        }
        out
    })
}

pub(super) fn reject(tokens: TokenStream) -> syn::Result<()> {
    map_sequences_reporting(tokens, |seq, errors| {
        for i in 0..seq.len() {
            if let Some((keyword, _)) = dyn_attr(seq, i) {
                combine(errors, syn::Error::new(keyword.span(), NEEDS_FEATURE));
            }
        }
        // Unchanged, so the enclosing sequence still sees the `[dyn]` groups.
        seq.to_vec()
    })
    .map(drop)
}

pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    let mut pass = Lower { errors: None };
    pass.visit_file_mut(file);
    pass.errors.map_or(Ok(()), Err)
}

/// The `dyn` keyword and the bracket group of a `#[dyn ..]` whose `#` is at `i`.
///
/// `dyn` is a keyword, so no valid attribute path is spelled `dyn` (`r#dyn` is a different
/// token): every such attribute is sugar.
fn dyn_attr(seq: &[TokenTree], i: usize) -> Option<(Ident, Group)> {
    let [hash, TokenTree::Group(group), ..] = &seq[i..] else {
        return None;
    };
    if !is_punct(hash, '#') || group.delimiter() != Delimiter::Bracket {
        return None;
    }
    match group.stream().into_iter().next()? {
        TokenTree::Ident(keyword) if keyword == "dyn" => Some((keyword, group.clone())),
        _ => None,
    }
}

/// Checks that the `#[dyn]` at `i` is the first one on its item and that the item is a trait.
fn on_one_trait(seq: &[TokenTree], i: usize, keyword: Span) -> syn::Result<()> {
    let first = outer_attributes_start(seq, i);
    if (first..i).step_by(2).any(|at| dyn_attr(seq, at).is_some()) {
        return Err(syn::Error::new(keyword, "`#[dyn]` is given twice"));
    }
    let mut j = i + 2;
    while matches!(&seq[j..], [hash, TokenTree::Group(g), ..]
        if is_punct(hash, '#') && g.delimiter() == Delimiter::Bracket)
    {
        j += 2;
    }
    let ident_at =
        |j: usize, name: &str| matches!(seq.get(j), Some(TokenTree::Ident(i)) if i == name);
    if ident_at(j, "pub") {
        j += 1;
        if matches!(seq.get(j), Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Parenthesis)
        {
            j += 1;
        }
    }
    while ident_at(j, "unsafe") || ident_at(j, "auto") {
        j += 1;
    }
    if ident_at(j, "trait") {
        Ok(())
    } else {
        Err(syn::Error::new(
            keyword,
            "`#[dyn]` goes on a trait, to make it usable as a dyn type",
        ))
    }
}

/// The inside of the marker for the attribute `group`, whose first token is `keyword`.
fn marker(keyword: &Ident, group: &Group) -> syn::Result<TokenStream> {
    let marker = Ident::new(MARKER, keyword.span());
    let rest: Vec<TokenTree> = group.stream().into_iter().skip(1).collect();
    match &rest[..] {
        [] => Ok(quote!(#marker)),
        [TokenTree::Group(args)] if args.delimiter() == Delimiter::Parenthesis => {
            let name = dyn_name(args)?;
            Ok(quote!(#marker(#name)))
        }
        [first, ..] => Err(syn::Error::new(first.span(), SHAPE)),
    }
}

/// The one name inside `#[dyn(..)]`; errors point at the first token that does not fit.
fn dyn_name(args: &Group) -> syn::Result<Ident> {
    let trees: Vec<TokenTree> = args.stream().into_iter().collect();
    // Parsed by `syn` so a keyword such as `pub` or `self` is not taken for a name.
    let name = trees
        .first()
        .and_then(|first| syn::parse2::<Ident>(first.clone().into()).ok());
    let offending = match (name, &trees[..]) {
        (Some(name), [_]) => return Ok(name),
        (_, []) => args.span(),
        (Some(_), [_, extra, ..]) => extra.span(),
        (None, [first, ..]) => first.span(),
    };
    Err(syn::Error::new(offending, SHAPE))
}

struct Lower {
    errors: Option<syn::Error>,
}

impl VisitMut for Lower {
    fn visit_item_trait_mut(&mut self, item: &mut ItemTrait) {
        if let Some(at) = item.attrs.iter().position(|a| a.path().is_ident(MARKER)) {
            match dynosaur_attr(item, &item.attrs[at]) {
                Ok(attr) => item.attrs[at] = attr,
                Err(err) => combine(&mut self.errors, err),
            }
        }
        visit_mut::visit_item_trait_mut(self, item);
    }
}

/// The `dynosaur` attribute that replaces `marker` on `item`.
fn dynosaur_attr(item: &ItemTrait, marker: &Attribute) -> syn::Result<Attribute> {
    let span = marker.path().span();
    let name = match &marker.meta {
        Meta::Path(_) => format_ident!("Dyn{}", item.ident, span = span),
        _ => marker.parse_args::<Ident>()?,
    };
    let (vis, ident) = (&item.vis, &item.ident);
    Ok(syn::parse_quote_spanned! {span=>
        #[::rustx::dynosaur::dynosaur(#vis #name = dyn(box) #ident)]
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;

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

    fn errors(sugar: TokenStream) -> Vec<String> {
        desugar(sugar)
            .err()
            .into_iter()
            .flatten()
            .map(|err| err.to_string())
            .collect()
    }

    #[test]
    fn dyn_trait_gets_the_dynosaur_attribute_with_the_default_name() {
        assert_eq!(
            desugar(quote! {
                #[dyn] pub trait Store: Send + Sync { async fn get(&self, key: &str) -> Option<u8>; }
            })
            .ok(),
            Some(reprint(quote! {
                #[::rustx::dynosaur::dynosaur(pub DynStore = dyn(box) Store)]
                pub trait Store: Send + Sync { async fn get(&self, key: &str) -> Option<u8>; }
            }))
        );
    }

    #[test]
    fn a_given_name_and_the_trait_visibility_are_used() {
        assert_eq!(
            desugar(quote! {
                #[dyn(AnyStore)] pub(crate) trait Store { async fn get(&self) -> u8; }
                #[dyn(r#Raw)] trait Private { fn f(&self); }
            })
            .ok(),
            Some(reprint(quote! {
                #[::rustx::dynosaur::dynosaur(pub(crate) AnyStore = dyn(box) Store)]
                pub(crate) trait Store { async fn get(&self) -> u8; }
                #[::rustx::dynosaur::dynosaur(r#Raw = dyn(box) Private)]
                trait Private { fn f(&self); }
            }))
        );
    }

    #[test]
    fn other_attributes_keep_their_order_around_the_marker() {
        assert_eq!(
            desugar(quote! {
                /// Docs.
                #[trait_variant::make(SendNext: Send)]
                #[dyn]
                #[allow(unused)]
                pub unsafe trait Next { async fn next(&mut self) -> Option<u8>; }
            })
            .ok(),
            Some(reprint(quote! {
                /// Docs.
                #[trait_variant::make(SendNext: Send)]
                #[::rustx::dynosaur::dynosaur(pub DynNext = dyn(box) Next)]
                #[allow(unused)]
                pub unsafe trait Next { async fn next(&mut self) -> Option<u8>; }
            }))
        );
    }

    #[test]
    fn traits_in_modules_and_function_bodies_are_lowered() {
        assert_eq!(
            desugar(quote! {
                mod m { #[dyn] pub trait A { async fn a(&self); } }
                fn f() { #[dyn] trait B { async fn b(&self); } }
            })
            .ok(),
            Some(reprint(quote! {
                mod m {
                    #[::rustx::dynosaur::dynosaur(pub DynA = dyn(box) A)]
                    pub trait A { async fn a(&self); }
                }
                fn f() {
                    #[::rustx::dynosaur::dynosaur(DynB = dyn(box) B)]
                    trait B { async fn b(&self); }
                }
            }))
        );
    }

    #[test]
    fn unmarked_traits_and_valid_rust_pass_through_unchanged() {
        let valid = quote! {
            pub trait Store { async fn get(&self) -> u8; }
            #[r#dyn] #[dyn_compatible] trait T { fn f(&self, x: &dyn Fn()) -> Box<dyn Store>; }
            fn g(x: &dyn T) {}
        };
        assert_eq!(desugar(valid.clone()).ok(), Some(reprint(valid)));
    }

    #[test]
    fn dyn_on_something_other_than_a_trait_is_an_error() {
        let message = "`#[dyn]` goes on a trait, to make it usable as a dyn type";
        for input in [
            quote! { #[dyn] struct S; },
            quote! { #[dyn] pub fn f() {} },
            quote! { #[dyn] impl T for S {} },
            quote! { trait T { #[dyn] async fn f(&self); } },
            quote! { #[dyn] },
        ] {
            assert_eq!(errors(input.clone()), [message], "{input}");
        }
    }

    #[test]
    fn a_malformed_argument_is_an_error() {
        let message = SHAPE;
        for input in [
            quote! { #[dyn()] trait T {} },
            quote! { #[dyn(A, B)] trait T {} },
            quote! { #[dyn(pub A)] trait T {} },
            quote! { #[dyn(self)] trait T {} },
            quote! { #[dyn("A")] trait T {} },
            quote! { #[dyn = A] trait T {} },
            quote! { #[dyn[A]] trait T {} },
            quote! { #[dyn(A) B] trait T {} },
        ] {
            assert_eq!(errors(input.clone()), [message], "{input}");
        }
    }

    #[test]
    fn dyn_given_twice_is_an_error_on_the_second() {
        assert_eq!(
            errors(quote! { #[dyn] #[cfg(all())] #[dyn(B)] trait T {} }),
            ["`#[dyn]` is given twice"]
        );
    }

    #[test]
    fn every_misuse_in_one_input_is_reported() {
        let errs = errors(quote! {
            #[dyn] struct S;
            mod m { #[dyn()] trait T {} #[dyn] #[dyn] trait U {} }
        });
        assert_eq!(errs.len(), 3, "{errs:?}");
    }

    #[test]
    fn marker_is_gone_after_lowering() {
        let out = desugar(quote! { #[dyn] trait T {} }).unwrap_or_default();
        assert!(
            !out.contains("__rustx") && out.contains("dynosaur"),
            "{out}"
        );
    }
}
