//! `error`: `Display` from per-variant messages, `std::error::Error`, `From` for `#[boil(from)]`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{DeriveInput, Fields};

use super::variants;
use crate::parse::parse_variant;

pub(crate) fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let mut arms = Vec::new();
    let mut froms = Vec::new();
    for v in variants(input)? {
        let cfg = parse_variant(v)?;
        let vid = &v.ident;
        let msg = cfg.msg.ok_or_else(|| {
            syn::Error::new_spanned(
                vid,
                "`error` pattern: every variant needs #[boil(msg = \"...\")]",
            )
        })?;
        let pat = match &v.fields {
            Fields::Unit => quote!(Self::#vid),
            Fields::Named(f) => {
                let ids = f.named.iter().map(|f| &f.ident);
                quote!(Self::#vid { #(#ids),* })
            }
            Fields::Unnamed(f) => {
                let ids = (0..f.unnamed.len()).map(|i| format_ident!("_{}", i));
                quote!(Self::#vid( #(#ids),* ))
            }
        };
        arms.push(quote!(#pat => ::core::write!(f, #msg)));
        if cfg.from {
            let Fields::Unnamed(f) = &v.fields else {
                return Err(syn::Error::new_spanned(
                    vid,
                    "#[boil(from)] needs a single-field tuple variant",
                ));
            };
            if f.unnamed.len() != 1 {
                return Err(syn::Error::new_spanned(
                    vid,
                    "#[boil(from)] needs a single-field tuple variant",
                ));
            }
            let ty = &f.unnamed[0].ty;
            froms.push(quote! {
                impl ::core::convert::From<#ty> for #name {
                    fn from(value: #ty) -> Self { Self::#vid(value) }
                }
            });
        }
    }
    let body = if arms.is_empty() {
        quote!(match *self {})
    } else {
        quote!(match self { #(#arms,)* })
    };
    Ok(quote! {
        impl ::core::fmt::Display for #name {
            #[allow(unused_variables)]
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { #body }
        }
        impl ::std::error::Error for #name {}
        #(#froms)*
    })
}
