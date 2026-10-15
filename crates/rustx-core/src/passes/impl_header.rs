//! The header of a trait impl the derive-like passes generate for a type they were given.
//!
//! Shared because every such impl must copy the type's generics and `where` clause the same
//! way, so the passes after them (`inline_bounds`, `#[borrows]`) extend them like hand-written
//! impls.

use proc_macro2::{Ident, Span, TokenStream};
use quote::quote_spanned;
use syn::Generics;

/// The type an impl is for.
pub(super) struct ImplHeader<'a> {
    pub(super) ident: &'a Ident,
    pub(super) generics: &'a Generics,
}

impl ImplHeader<'_> {
    /// `impl<..> tr for Type<..> where .. { body }`, spanned at `span`.
    pub(super) fn implement(
        &self,
        span: Span,
        tr: &TokenStream,
        body: &TokenStream,
    ) -> TokenStream {
        let ident = self.ident;
        let (impl_generics, type_generics, where_clause) = self.generics.split_for_impl();
        quote_spanned! {span=>
            impl #impl_generics #tr for #ident #type_generics #where_clause { #body }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn the_impl_copies_generics_and_the_where_clause() {
        let item: syn::ItemStruct = syn::parse2(quote!(
            struct W<'a, T: Clone>
            where
                T: Send,
            {
                t: &'a T,
            }
        ))
        .unwrap();
        let header = ImplHeader {
            ident: &item.ident,
            generics: &item.generics,
        };
        let generated = header.implement(
            Span::call_site(),
            &quote!(Tr),
            &quote!(
                fn f() {}
            ),
        );
        assert_eq!(
            generated.to_string(),
            quote!(
                impl<'a, T: Clone> Tr for W<'a, T>
                where
                    T: Send,
                {
                    fn f() {}
                }
            )
            .to_string()
        );
    }
}
