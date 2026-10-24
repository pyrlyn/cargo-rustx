//! `builder`: `<Name>Builder` with chained setters and a fallible `build()`.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::DeriveInput;

use super::named_fields;

pub(crate) fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let vis = &input.vis;
    let builder = format_ident!("{}Builder", name);
    let generics = &input.generics;
    let (impl_g, ty_g, where_c) = generics.split_for_impl();
    let fields = named_fields(input)?;

    let mut slots = Vec::new();
    let mut nones = Vec::new();
    let mut setters = Vec::new();
    let mut builds = Vec::new();
    for (f, cfg) in &fields {
        let id = f.ident.as_ref().expect("named");
        let ty = &f.ty;
        if cfg.skip {
            let value = cfg.default.as_ref().map_or_else(
                || quote!(::core::default::Default::default()),
                |d| quote!(#d),
            );
            builds.push(quote!(#id: #value));
            continue;
        }
        slots.push(quote!(#id: ::core::option::Option<#ty>));
        nones.push(quote!(#id: ::core::option::Option::None));
        let doc = format!("Set `{id}`.");
        setters.push(quote! {
            #[doc = #doc]
            #[must_use]
            pub fn #id(mut self, value: impl ::core::convert::Into<#ty>) -> Self {
                self.#id = ::core::option::Option::Some(value.into());
                self
            }
        });
        builds.push(match &cfg.default {
            Some(d) => quote!(#id: self.#id.unwrap_or_else(|| #d)),
            None => {
                let msg = format!("missing field `{id}`");
                quote!(#id: self.#id.ok_or_else(|| ::std::string::String::from(#msg))?)
            }
        });
    }
    let doc = format!("Builder for [`{name}`].");
    Ok(quote! {
        #[doc = #doc]
        #vis struct #builder #generics #where_c { #(#slots,)* }

        impl #impl_g ::core::default::Default for #builder #ty_g #where_c {
            fn default() -> Self { Self { #(#nones,)* } }
        }

        impl #impl_g #builder #ty_g #where_c {
            #(#setters)*
            /// Build the value; fails on the first missing required field.
            pub fn build(self) -> ::core::result::Result<#name #ty_g, ::std::string::String> {
                ::core::result::Result::Ok(#name { #(#builds,)* })
            }
        }

        impl #impl_g #name #ty_g #where_c {
            /// Start a builder.
            pub fn builder() -> #builder #ty_g { ::core::default::Default::default() }
        }
    })
}
