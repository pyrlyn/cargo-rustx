//! `convert`: `From` both ways between a newtype and its inner value.

use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

use super::newtype_inner;

pub(crate) fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let inner = newtype_inner(input)?;
    Ok(quote! {
        impl ::core::convert::From<#inner> for #name {
            fn from(value: #inner) -> Self { Self(value) }
        }
        impl ::core::convert::From<#name> for #inner {
            fn from(value: #name) -> Self { value.0 }
        }
    })
}
