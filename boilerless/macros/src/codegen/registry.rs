//! `registry`: `ALL`, `COUNT`, `index()`, `variant_name()` for unit enums.

use proc_macro2::TokenStream;
use quote::quote;
use syn::DeriveInput;

use super::variants;

pub(crate) fn expand(input: &DeriveInput) -> syn::Result<TokenStream> {
    let name = &input.ident;
    let vs = variants(input)?;
    let ids: Vec<_> = vs.iter().map(|v| &v.ident).collect();
    let count = ids.len();
    let idx = (0..count).map(|i| quote!(#i));
    let strs = ids.iter().map(ToString::to_string);
    let (index_body, name_body) = if ids.is_empty() {
        (quote!(match *self {}), quote!(match *self {}))
    } else {
        (
            quote!(match self { #(Self::#ids => #idx,)* }),
            quote!(match self { #(Self::#ids => #strs,)* }),
        )
    };
    Ok(quote! {
        impl #name {
            /// Every variant, in declaration order.
            pub const ALL: &'static [Self] = &[#(Self::#ids),*];
            /// Number of variants.
            pub const COUNT: usize = #count;
            /// Position of this variant in [`Self::ALL`].
            pub const fn index(&self) -> usize { #index_body }
            /// The Rust identifier of this variant.
            pub const fn variant_name(&self) -> &'static str { #name_body }
        }
    })
}
