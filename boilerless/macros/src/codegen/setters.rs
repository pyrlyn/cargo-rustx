//! `setters`: `fn set_field(&mut self, v) -> &mut Self`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::DeriveInput;

use super::named_fields;

pub(crate) fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let (impl_g, ty_g, where_c) = input.generics.split_for_impl();
    let setters = named_fields(input)?
        .into_iter()
        .filter(|(_, c)| !c.skip)
        .map(|(f, _)| {
            let id = f.ident.as_ref().expect("named");
            let set = format_ident!("set_{}", id);
            let ty = &f.ty;
            let doc = format!("Set `{id}`.");
            quote! {
                #[doc = #doc]
                pub fn #set(&mut self, value: impl ::core::convert::Into<#ty>) -> &mut Self {
                    self.#id = value.into();
                    self
                }
            }
        });
    Ok(quote! { impl #impl_g #name #ty_g #where_c { #(#setters)* } })
}
