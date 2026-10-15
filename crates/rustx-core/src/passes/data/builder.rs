//! `#[data(builder)]`: a `<Name>Builder` struct with one method per field and a checked `build`.

use std::collections::HashSet;

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote_spanned};
use syn::{Ident, Item, LitStr};

use super::{Data, DataField};

/// Builder methods that a field of the same name would collide with.
const RESERVED: &[&str] = &["build", "builder"];

/// Why the struct cannot have a builder, if it cannot.
pub(super) fn check(
    ident: &Ident,
    fields: &[DataField],
    at: Span,
    declared: &HashSet<String>,
) -> syn::Result<()> {
    let name = name(ident, at);
    if declared.contains(&name.to_string()) {
        let msg = format!(
            "`{name}` is already declared next to `{ident}`, so `#[data(builder)]` cannot add it"
        );
        return Err(syn::Error::new(at, msg));
    }
    for field in fields {
        if RESERVED.iter().any(|reserved| field.ident == reserved) {
            let msg = format!(
                "a field named `{}` collides with the `{0}` method `#[data(builder)]` adds",
                field.ident
            );
            return Err(syn::Error::new(field.ident.span(), msg));
        }
    }
    Ok(())
}

fn name(ident: &Ident, at: Span) -> Ident {
    format_ident!("{}Builder", ident, span = at)
}

/// `pub fn builder()` for the struct's own `impl`, when the flag is set.
pub(super) fn entry(data: &Data) -> Option<TokenStream> {
    let at = data.flags.builder?;
    let name = name(&data.item.ident, at);
    let (_, ty_generics, _) = data.item.generics.split_for_impl();
    let fields = data.fields.iter().map(|field| &field.ident);
    Some(quote_spanned! {at=>
        pub fn builder() -> #name #ty_generics {
            #name { #(#fields: ::core::option::Option::None),* }
        }
    })
}

/// The builder struct and its `impl`.
pub(super) fn generate(data: &Data) -> syn::Result<Vec<Item>> {
    let Some(at) = data.flags.builder else {
        return Ok(Vec::new());
    };
    let item = data.item;
    let target = &item.ident;
    let name = name(target, at);
    let vis = &item.vis;
    let generics = &item.generics;
    let (impl_generics, ty_generics, where_clause) = generics.split_for_impl();
    // `#[borrows]` fills the builder's own fields, like the struct's, when it runs.
    let borrows = data
        .lifetime
        .as_ref()
        .map(|_| quote_spanned!(at=> #[borrows]));
    let slots = data.fields.iter().map(|field| {
        let (field, ty) = (&field.ident, &field.ty);
        quote_spanned!(field.span()=> #field: ::core::option::Option<#ty>)
    });
    let setters = data.fields.iter().map(|field| {
        let (name, ty) = (&field.ident, data.signature_type(&field.ty));
        quote_spanned! {name.span()=>
            pub fn #name(mut self, #name: #ty) -> Self {
                self.#name = ::core::option::Option::Some(#name);
                self
            }
        }
    });
    let values = data.fields.iter().map(|field| value(target, field));
    let tokens = quote_spanned! {at=>
        #borrows
        #vis struct #name #generics #where_clause {
            #(#slots),*
        }
        #[allow(dead_code)]
        impl #impl_generics #name #ty_generics #where_clause {
            #(#setters)*
            pub fn build(self) -> ::core::result::Result<#target #ty_generics, &'static str> {
                ::core::result::Result::Ok(#target { #(#values),* })
            }
        }
    };
    Ok(syn::parse2::<syn::File>(tokens)?.items)
}

/// A field of the built struct: the value set, else its default, else an error.
fn value(target: &Ident, field: &DataField) -> TokenStream {
    let name = &field.ident;
    if let Some(default) = &field.default {
        quote_spanned!(name.span()=> #name: self.#name.unwrap_or_else(|| #default))
    } else {
        let msg = format!("`{target}` builder is missing `{name}`");
        let msg = LitStr::new(&msg, name.span());
        quote_spanned!(name.span()=> #name: self.#name.ok_or(#msg)?)
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{errors, lowered, printed};
    use quote::quote;

    #[test]
    fn builder_adds_an_entry_a_builder_struct_and_a_checked_build() {
        assert_eq!(
            lowered(quote! {
                #[data(builder)]
                pub(crate) struct Opts<T: Clone> where T: Default { name: String, #[default(3)] n: T }
            })
            .unwrap(),
            printed(quote! {
                pub(crate) struct Opts<T: Clone> where T: Default { name: String, n: T }
                #[allow(dead_code)]
                impl<T: Clone> Opts<T> where T: Default {
                    pub fn new(name: String) -> Self { Self { name, n: 3 } }
                    pub fn name(&self) -> &str { &self.name }
                    pub fn n(&self) -> &T { &self.n }
                    pub fn builder() -> OptsBuilder<T> {
                        OptsBuilder {
                            name: ::core::option::Option::None,
                            n: ::core::option::Option::None
                        }
                    }
                }
                pub(crate) struct OptsBuilder<T: Clone> where T: Default {
                    name: ::core::option::Option<String>,
                    n: ::core::option::Option<T>
                }
                #[allow(dead_code)]
                impl<T: Clone> OptsBuilder<T> where T: Default {
                    pub fn name(mut self, name: String) -> Self {
                        self.name = ::core::option::Option::Some(name);
                        self
                    }
                    pub fn n(mut self, n: T) -> Self {
                        self.n = ::core::option::Option::Some(n);
                        self
                    }
                    pub fn build(self) -> ::core::result::Result<Opts<T>, &'static str> {
                        ::core::result::Result::Ok(Opts {
                            name: self.name.ok_or("`Opts` builder is missing `name`")?,
                            n: self.n.unwrap_or_else(|| 3)
                        })
                    }
                }
            })
        );
    }

    #[test]
    fn a_borrows_struct_gets_a_borrows_builder() {
        let expanded = lowered(quote! {
            #[borrows]
            #[data(builder)]
            struct Line { text: &str }
        })
        .unwrap();
        assert!(
            expanded.contains(&printed(quote! {
                #[borrows] struct LineBuilder { text: ::core::option::Option<&str> }
            })),
            "{expanded}"
        );
        assert!(
            expanded.contains("pub fn text (mut self , text : & '__rustx_borrows str)"),
            "{expanded}"
        );
    }

    #[test]
    fn a_struct_that_cannot_have_a_builder_is_an_error() {
        assert_eq!(
            errors(quote! {
                #[data(builder)] struct A { build: u8 }
                #[data(builder)] struct B { builder: u8 }
                mod m {
                    #[data(builder)] struct C { c: u8 }
                    enum CBuilder {}
                }
                #[data(builder)] struct D { d: u8 }
            }),
            [
                "a field named `build` collides with the `build` method `#[data(builder)]` adds",
                "a field named `builder` collides with the `builder` method `#[data(builder)]` adds",
                "`CBuilder` is already declared next to `C`, so `#[data(builder)]` cannot add it",
            ]
        );
    }
}
