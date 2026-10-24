//! `config`: load a struct from env-like key/value sources.

use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;
use syn::ext::IdentExt;

use super::named_fields;

pub(crate) fn expand(input: &DeriveInput, prefix: &str) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let inits = named_fields(input)?.into_iter().map(|(f, cfg)| {
        let id = f.ident.as_ref().expect("named");
        let ty = &f.ty;
        let key = format!("{prefix}{}", id.unraw().to_string().to_uppercase());
        let missing = match &cfg.default {
            Some(d) => quote!(#d),
            None => {
                let msg = format!("missing config key `{key}`");
                quote!(return ::core::result::Result::Err(::std::string::String::from(#msg)))
            }
        };
        quote! {
            #id: match lookup(#key) {
                ::core::option::Option::Some(raw) => raw
                    .parse::<#ty>()
                    .map_err(|e| ::std::format!("invalid `{}`: {}", #key, e))?,
                ::core::option::Option::None => #missing,
            }
        }
    });
    Ok(quote! {
        impl #name {
            /// Load from the process environment.
            pub fn from_env() -> ::core::result::Result<Self, ::std::string::String> {
                Self::from_lookup(|key| ::std::env::var(key).ok())
            }
            /// Load from any key -> value source.
            pub fn from_lookup(
                lookup: impl ::core::ops::Fn(&str) -> ::core::option::Option<::std::string::String>,
            ) -> ::core::result::Result<Self, ::std::string::String> {
                ::core::result::Result::Ok(Self { #(#inits,)* })
            }
        }
    })
}
