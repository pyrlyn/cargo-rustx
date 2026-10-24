//! `newtype`: delegate to the inner value via `Deref`/`DerefMut`/`AsRef`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

use super::newtype_inner;

pub(crate) fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let inner = newtype_inner(input)?;
    Ok(quote! {
        impl ::core::ops::Deref for #name {
            type Target = #inner;
            fn deref(&self) -> &Self::Target { &self.0 }
        }
        impl ::core::ops::DerefMut for #name {
            fn deref_mut(&mut self) -> &mut Self::Target { &mut self.0 }
        }
        impl ::core::convert::AsRef<#inner> for #name {
            fn as_ref(&self) -> &#inner { &self.0 }
        }
        impl #name {
            /// Unwrap the inner value.
            pub fn into_inner(self) -> #inner { self.0 }
        }
    })
}
