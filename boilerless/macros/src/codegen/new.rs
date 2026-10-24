//! `new`: positional constructor over required fields.

use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

use super::named_fields;

pub(crate) fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let (impl_g, ty_g, where_c) = input.generics.split_for_impl();
    let mut params = Vec::new();
    let mut inits = Vec::new();
    for (f, cfg) in named_fields(input)? {
        let id = f.ident.as_ref().expect("named");
        let ty = &f.ty;
        match (&cfg.default, cfg.skip) {
            (Some(d), _) => inits.push(quote!(#id: #d)),
            (None, true) => inits.push(quote!(#id: ::core::default::Default::default())),
            (None, false) => {
                params.push(quote!(#id: impl ::core::convert::Into<#ty>));
                inits.push(quote!(#id: #id.into()));
            }
        }
    }
    Ok(quote! {
        impl #impl_g #name #ty_g #where_c {
            /// Construct from the required fields; the rest take their defaults.
            #[allow(clippy::too_many_arguments)]
            pub fn new(#(#params),*) -> Self { Self { #(#inits,)* } }
        }
    })
}
