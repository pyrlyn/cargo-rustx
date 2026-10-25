//! `enum_str`: `as_str`, `Display`, `FromStr` for unit enums.

use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;
use syn::ext::IdentExt;

use super::variants;
use crate::parse::parse_variant;

pub(crate) fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let mut to = Vec::new();
    let mut from = Vec::new();
    for v in variants(input)? {
        let vid = &v.ident;
        let s = parse_variant(v)?
            .rename
            .map_or_else(|| vid.unraw().to_string(), |r| r.value());
        to.push(quote!(Self::#vid => #s));
        from.push(quote!(#s => ::core::result::Result::Ok(Self::#vid)));
    }
    let to_body = if to.is_empty() {
        quote!(match *self {})
    } else {
        quote!(match self { #(#to,)* })
    };
    let err = format!("unknown {name} variant: ");
    Ok(quote! {
        impl #name {
            /// The string form of this variant.
            pub const fn as_str(&self) -> &'static str { #to_body }
        }
        impl ::core::fmt::Display for #name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { f.write_str(self.as_str()) }
        }
        impl ::core::str::FromStr for #name {
            type Err = ::std::string::String;
            fn from_str(s: &str) -> ::core::result::Result<Self, Self::Err> {
                match s {
                    #(#from,)*
                    other => ::core::result::Result::Err(::std::format!("{}{}", #err, other)),
                }
            }
        }
    })
}
