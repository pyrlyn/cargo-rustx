//! `getters`: `fn field(&self) -> &Ty`.

use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

use super::named_fields;

pub(crate) fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let (impl_g, ty_g, where_c) = input.generics.split_for_impl();
    let getters = named_fields(input)?
        .into_iter()
        .filter(|(_, c)| !c.skip)
        .map(|(f, _)| {
            let id = f.ident.as_ref().expect("named");
            let ty = &f.ty;
            let doc = format!("Borrow `{id}`.");
            quote! {
                #[doc = #doc]
                pub fn #id(&self) -> &#ty { &self.#id }
            }
        });
    Ok(quote! { impl #impl_g #name #ty_g #where_c { #(#getters)* } })
}
