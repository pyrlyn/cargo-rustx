//! The `impl` of a `#[data]` struct: `new`, getters, and the `setters` and `with` flags.

use proc_macro2::TokenStream;
use quote::{format_ident, quote_spanned};
use syn::{Item, Type};

use super::{Data, DataField, builder};
use crate::tokens::syntax::is_string;

/// Primitive types whose getters return the value.
const COPY: &[&str] = &[
    "bool", "char", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128",
    "isize", "f32", "f64",
];

pub(super) fn generate(data: &Data) -> syn::Result<Item> {
    let ident = &data.item.ident;
    let (impl_generics, ty_generics, where_clause) = data.item.generics.split_for_impl();
    let params = data
        .fields
        .iter()
        .filter(|field| field.default.is_none())
        .map(|field| {
            let (name, ty) = (&field.ident, data.signature_type(&field.ty));
            quote_spanned!(name.span()=> #name: #ty)
        });
    let inits = data.fields.iter().map(|field| {
        let name = &field.ident;
        if let Some(value) = &field.default {
            quote_spanned!(name.span()=> #name: #value)
        } else {
            quote_spanned!(name.span()=> #name)
        }
    });
    let getters = data.fields.iter().map(|field| getter(data, field));
    let setters = data
        .flags
        .setters
        .then(|| data.fields.iter().map(|field| setter(data, field)))
        .into_iter()
        .flatten();
    let withs = data
        .flags
        .with
        .then(|| data.fields.iter().map(|field| with(data, field)))
        .into_iter()
        .flatten();
    let builder = builder::entry(data);
    syn::parse2(quote_spanned! {data.span=>
        #[allow(dead_code)]
        impl #impl_generics #ident #ty_generics #where_clause {
            pub fn new(#(#params),*) -> Self {
                Self { #(#inits),* }
            }
            #(#getters)*
            #(#setters)*
            #(#withs)*
            #builder
        }
    })
}

fn getter(data: &Data, field: &DataField) -> TokenStream {
    let name = &field.ident;
    let ty = data.signature_type(&field.ty);
    if is_copy(&ty) {
        quote_spanned!(name.span()=> pub fn #name(&self) -> #ty { self.#name })
    } else if is_string(&ty) {
        quote_spanned!(name.span()=> pub fn #name(&self) -> &str { &self.#name })
    } else {
        quote_spanned!(name.span()=> pub fn #name(&self) -> &#ty { &self.#name })
    }
}

fn setter(data: &Data, field: &DataField) -> TokenStream {
    let name = &field.ident;
    let ty = data.signature_type(&field.ty);
    let method = format_ident!("set_{}", name, span = name.span());
    quote_spanned! {name.span()=>
        pub fn #method(&mut self, #name: #ty) {
            self.#name = #name;
        }
    }
}

fn with(data: &Data, field: &DataField) -> TokenStream {
    let name = &field.ident;
    let ty = data.signature_type(&field.ty);
    let method = format_ident!("with_{}", name, span = name.span());
    quote_spanned! {name.span()=>
        pub fn #method(mut self, #name: #ty) -> Self {
            self.#name = #name;
            self
        }
    }
}

/// A primitive or a shared reference: cheap to copy, so the getter returns the value.
fn is_copy(ty: &Type) -> bool {
    match ty {
        Type::Reference(reference) => reference.mutability.is_none(),
        Type::Path(path) if path.qself.is_none() => path
            .path
            .get_ident()
            .is_some_and(|name| COPY.iter().any(|copy| name == copy)),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{lowered, printed};
    use super::*;
    use quote::quote;

    #[test]
    fn the_problem_example_generates_new_getters_setters_and_withs() {
        assert_eq!(
            lowered(quote! {
                #[data(setters, with)]
                pub struct User {
                    id: u64,
                    name: String,
                    #[doc = "kept"]
                    #[default(true)]
                    active: bool,
                }
            })
            .unwrap(),
            printed(quote! {
                pub struct User {
                    id: u64,
                    name: String,
                    #[doc = "kept"]
                    active: bool,
                }
                #[allow(dead_code)]
                impl User {
                    pub fn new(id: u64, name: String) -> Self {
                        Self { id, name, active: true }
                    }
                    pub fn id(&self) -> u64 { self.id }
                    pub fn name(&self) -> &str { &self.name }
                    pub fn active(&self) -> bool { self.active }
                    pub fn set_id(&mut self, id: u64) { self.id = id; }
                    pub fn set_name(&mut self, name: String) { self.name = name; }
                    pub fn set_active(&mut self, active: bool) { self.active = active; }
                    pub fn with_id(mut self, id: u64) -> Self { self.id = id; self }
                    pub fn with_name(mut self, name: String) -> Self { self.name = name; self }
                    pub fn with_active(mut self, active: bool) -> Self {
                        self.active = active;
                        self
                    }
                }
            })
        );
    }

    #[test]
    fn generics_and_where_are_copied_to_the_impl() {
        assert_eq!(
            lowered(quote! {
                #[data]
                struct Pair<'a, K: Clone, V = u8, const N: usize = 1> where V: Copy {
                    key: &'a K,
                    values: [V; N],
                }
            })
            .unwrap(),
            printed(quote! {
                struct Pair<'a, K: Clone, V = u8, const N: usize = 1> where V: Copy {
                    key: &'a K,
                    values: [V; N],
                }
                #[allow(dead_code)]
                impl<'a, K: Clone, V, const N: usize> Pair<'a, K, V, N> where V: Copy {
                    pub fn new(key: &'a K, values: [V; N]) -> Self { Self { key, values } }
                    pub fn key(&self) -> &'a K { self.key }
                    pub fn values(&self) -> &[V; N] { &self.values }
                }
            })
        );
    }

    #[test]
    fn a_borrows_struct_names_its_lifetime_in_parameters_and_copied_results() {
        let expanded = lowered(quote! {
            #[data(setters)]
            #[borrows]
            struct Line { text: &str, words: Vec<&str>, n: usize }
        })
        .unwrap();
        let expected = printed(quote! {
            #[borrows]
            struct Line { text: &str, words: Vec<&str>, n: usize }
            #[allow(dead_code)]
            impl Line {
                pub fn new(
                    text: &'__rustx_borrows str,
                    words: Vec<&'__rustx_borrows str>,
                    n: usize
                ) -> Self {
                    Self { text, words, n }
                }
                pub fn text(&self) -> &'__rustx_borrows str { self.text }
                pub fn words(&self) -> &Vec<&'__rustx_borrows str> { &self.words }
                pub fn n(&self) -> usize { self.n }
                pub fn set_text(&mut self, text: &'__rustx_borrows str) { self.text = text; }
                pub fn set_words(&mut self, words: Vec<&'__rustx_borrows str>) {
                    self.words = words;
                }
                pub fn set_n(&mut self, n: usize) { self.n = n; }
            }
        });
        assert_eq!(expanded, expected);
    }

    #[test]
    fn copy_and_string_getters_are_judged_by_the_written_type() {
        let ty = |source: &str| syn::parse_str::<Type>(source).unwrap();
        for copy in ["u8", "f64", "bool", "char", "&T", "&[u8]"] {
            assert!(is_copy(&ty(copy)), "{copy}");
        }
        for not_copy in ["String", "&mut T", "Id", "core::primitive::u8", "(u8, u8)"] {
            assert!(!is_copy(&ty(not_copy)), "{not_copy}");
        }
        for string in ["String", "::std::string::String", "alloc::string::String"] {
            assert!(is_string(&ty(string)), "{string}");
        }
        for not_string in ["my::String", "String<u8>", "str", "&String"] {
            assert!(!is_string(&ty(not_string)), "{not_string}");
        }
    }
}
