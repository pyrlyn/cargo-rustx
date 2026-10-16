//! Walks every fn signature of a file together with its body, when it has one.
//!
//! Shared by the parameter passes: sugar on a parameter must be lowered on free fns, methods,
//! trait items and `extern` fns alike, and a pass that missed one kind would leave its marker
//! behind for rustc to reject.

use syn::visit_mut::{self, VisitMut};
use syn::{Attribute, Block, ForeignItemFn, ImplItemFn, ItemFn, Signature, TraitItemFn};

use crate::tokens::syntax::combine;

/// Calls `rewrite` on every fn signature, nested fns included, outer before inner.
///
/// `body` is `None` for a trait method without a default body and for an `extern` fn. Errors
/// from all signatures are combined.
pub(super) fn for_each_signature<F>(file: &mut syn::File, mut rewrite: F) -> syn::Result<()>
where
    F: FnMut(&mut Signature, Option<&mut Block>) -> syn::Result<()>,
{
    for_each_fn(file, |_, sig, body| rewrite(sig, body))
}

/// Like [`for_each_signature`], with the fn's attributes too, inner ones from its body included:
/// a pass that marks the whole fn rather than a parameter finds its marker there.
pub(super) fn for_each_fn<F>(file: &mut syn::File, rewrite: F) -> syn::Result<()>
where
    F: FnMut(&mut Vec<Attribute>, &mut Signature, Option<&mut Block>) -> syn::Result<()>,
{
    let mut walk = Walk {
        rewrite,
        errors: None,
    };
    walk.visit_file_mut(file);
    walk.errors.map_or(Ok(()), Err)
}

struct Walk<F> {
    rewrite: F,
    errors: Option<syn::Error>,
}

impl<F> Walk<F>
where
    F: FnMut(&mut Vec<Attribute>, &mut Signature, Option<&mut Block>) -> syn::Result<()>,
{
    fn apply(&mut self, attrs: &mut Vec<Attribute>, sig: &mut Signature, body: Option<&mut Block>) {
        if let Err(err) = (self.rewrite)(attrs, sig, body) {
            combine(&mut self.errors, err);
        }
    }
}

impl<F> VisitMut for Walk<F>
where
    F: FnMut(&mut Vec<Attribute>, &mut Signature, Option<&mut Block>) -> syn::Result<()>,
{
    fn visit_item_fn_mut(&mut self, item: &mut ItemFn) {
        self.apply(&mut item.attrs, &mut item.sig, Some(&mut item.block));
        visit_mut::visit_item_fn_mut(self, item);
    }

    fn visit_impl_item_fn_mut(&mut self, item: &mut ImplItemFn) {
        self.apply(&mut item.attrs, &mut item.sig, Some(&mut item.block));
        visit_mut::visit_impl_item_fn_mut(self, item);
    }

    fn visit_trait_item_fn_mut(&mut self, item: &mut TraitItemFn) {
        self.apply(&mut item.attrs, &mut item.sig, item.default.as_mut());
        visit_mut::visit_trait_item_fn_mut(self, item);
    }

    fn visit_foreign_item_fn_mut(&mut self, item: &mut ForeignItemFn) {
        self.apply(&mut item.attrs, &mut item.sig, None);
        visit_mut::visit_foreign_item_fn_mut(self, item);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro2::Span;
    use quote::quote;

    #[test]
    fn every_fn_kind_is_visited_outer_first_with_its_body() {
        let mut file: syn::File = syn::parse2(quote! {
            fn free() { fn nested() {} }
            impl S { fn method(&self) {} }
            trait T { fn declared(); fn defaulted() {} }
            extern "C" { fn foreign(); }
        })
        .unwrap();
        let mut seen = Vec::new();
        for_each_signature(&mut file, |sig, body| {
            seen.push(format!("{} {}", sig.ident, body.is_some()));
            Ok(())
        })
        .unwrap();
        assert_eq!(
            seen,
            [
                "free true",
                "nested true",
                "method true",
                "declared false",
                "defaulted true",
                "foreign false",
            ]
        );
    }

    #[test]
    fn every_fn_comes_with_its_outer_and_inner_attributes() {
        let mut file: syn::File = syn::parse2(quote! {
            #[a] fn free() { #![b] }
            impl S { #[c] fn method(&self) { #![d] } }
            trait T { #[e] fn declared(); }
        })
        .unwrap();
        let mut seen = Vec::new();
        for_each_fn(&mut file, |attrs, sig, _| {
            let names = attrs
                .iter()
                .map(|attr| quote::ToTokens::to_token_stream(attr.path()).to_string());
            seen.push(format!(
                "{} {}",
                sig.ident,
                names.collect::<Vec<_>>().join(",")
            ));
            Ok(())
        })
        .unwrap();
        assert_eq!(seen, ["free a,b", "method c,d", "declared e"]);
    }

    #[test]
    fn errors_from_every_signature_are_combined() {
        let mut file: syn::File = syn::parse2(quote! { fn a() {} fn b() {} }).unwrap();
        let err = for_each_signature(&mut file, |sig, _| {
            Err(syn::Error::new(Span::call_site(), sig.ident.to_string()))
        })
        .unwrap_err();
        let messages: Vec<String> = err.into_iter().map(|err| err.to_string()).collect();
        assert_eq!(messages, ["a", "b"]);
    }
}
