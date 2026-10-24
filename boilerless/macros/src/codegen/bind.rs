//! `bind`: implement a trait by delegating to the methods of an inherent impl block.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{FnArg, ImplItem, ItemImpl, Pat, Path};

pub(crate) fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    if attr.is_empty() {
        return Err(syn::Error::new(
            proc_macro2::Span::call_site(),
            "#[bind] needs a trait path: #[bind(path::to::Trait)]",
        ));
    }
    let tr: Path = syn::parse2(attr)?;
    let block: ItemImpl = syn::parse2(item)?;
    if let Some((_, path, _)) = &block.trait_ {
        return Err(syn::Error::new_spanned(
            path,
            "#[bind] goes on an inherent impl block, not a trait impl",
        ));
    }
    let self_ty = &block.self_ty;
    let (impl_g, _, where_c) = block.generics.split_for_impl();
    let mut methods = Vec::new();
    for it in &block.items {
        let ImplItem::Fn(f) = it else { continue };
        let mut sig = f.sig.clone();
        let mut args = Vec::new();
        for (i, a) in sig.inputs.iter_mut().enumerate() {
            match a {
                FnArg::Receiver(_) => args.push(quote!(self)),
                FnArg::Typed(t) => match &*t.pat {
                    Pat::Ident(p) => {
                        let id = &p.ident;
                        args.push(quote!(#id));
                    }
                    _ => {
                        let id = quote::format_ident!("__arg{}", i);
                        *t.pat = syn::parse_quote!(#id);
                        args.push(quote!(#id));
                    }
                },
            }
        }
        let fname = &sig.ident;
        let call = quote!(<#self_ty>::#fname(#(#args),*));
        let call = if sig.asyncness.is_some() {
            quote!(#call.await)
        } else {
            call
        };
        methods.push(quote!(#sig { #call }));
    }
    Ok(quote! {
        #block
        impl #impl_g #tr for #self_ty #where_c { #(#methods)* }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bind_codegen() {
        let out = expand(
            quote!(Greet),
            quote!(impl Dog { pub fn name(&self) -> String { "rex".into() } fn add(&self, (a, b): (u8, u8)) -> u8 { a + b } }),
        )
        .unwrap()
        .to_string()
        .replace(' ', "");
        assert!(out.contains("implGreetforDog"), "{out}");
        assert!(out.contains("<Dog>::name(self)"), "{out}");
        assert!(out.contains("<Dog>::add(self,__arg1)"), "{out}");
        assert!(
            out.contains("pubfnname(&self)->String{\"rex\""),
            "original kept: {out}"
        );
    }

    #[test]
    fn bind_errors() {
        assert!(
            expand(quote!(), quote!(impl Dog {}))
                .unwrap_err()
                .to_string()
                .contains("trait path")
        );
        assert!(expand(quote!(Greet), quote!(impl Greet for Dog {})).is_err());
        assert!(expand(quote!(1 + 2), quote!(impl Dog {})).is_err());
        assert!(
            expand(
                quote!(Greet),
                quote!(
                    fn not_impl() {}
                )
            )
            .is_err()
        );
    }
}
