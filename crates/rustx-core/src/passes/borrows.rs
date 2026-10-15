//! `#[borrows]` types: one lifetime added to the type, its fields and its `impl` headers (problem 19).
//!
//! An AST pass: references without a lifetime in a struct still parse, so the rewrite happens after
//! parsing, before `from_lifetimes` resolves `from(Self)` to the type's lifetime.
//!
//! `#[borrows]` on a struct or enum adds the lifetime `'__rustx_borrows` (see [`lifetime`]):
//!
//! - first in the type's generics;
//! - on every elided reference and `'_` in its fields, at any depth;
//! - to every borrowing type in its fields written without a lifetime argument
//!   (`Peekable<Lexer>` → `Peekable<Lexer<'__rustx_borrows>>`).
//!
//! An `impl` whose header names a borrowing type without a lifetime argument (`impl Lexer`,
//! `impl Iterator for Lexer`, `impl<T> Wrap<T>`) gets the lifetime first in its generics, and so do
//! the borrowing types in that header and in the signatures, associated types and constants inside
//! it. Elided references there stay elided, so `&str` on a `&mut self` method still borrows from
//! the receiver; `&from(Self) str` names the type's lifetime instead.
//!
//! # Hand-off to `from_lifetimes`
//!
//! The attribute is gone once this pass ran, so the added lifetime is the record: a type or `impl`
//! whose generics declare it is a borrowing one ([`has_lifetime`], [`borrowing_types`]). Every
//! borrowing type shares the one name, so inside an `impl` the lifetime is in scope for all of them.
//!
//! # Limitations
//!
//! - Exactly one lifetime: a type that already declares one is reported, not extended.
//! - Types are matched by their last path segment, across the whole input, without module scopes.
//! - Function bodies are left alone: local types infer their lifetimes as usual.
//! - `fn(&T)` and `Fn(&T)` types keep their own elision, as in signatures.

use std::collections::HashSet;

use proc_macro2::{Ident, Span};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit_mut::{self, VisitMut};
use syn::{
    AngleBracketedGenericArguments, AttrStyle, Attribute, Field, GenericArgument, GenericParam,
    Generics, ImplItem, ItemEnum, ItemImpl, ItemStruct, Lifetime, LifetimeParam, Meta,
    ParenthesizedGenericArguments, PathArguments, Token, TypeFnPtr, TypePath, TypeReference,
};

use crate::tokens::syntax::combine;
use crate::tokens::syntax::introduced_span;

/// The name of the one lifetime this pass adds.
const NAME: &str = "__rustx_borrows";

pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    // Fields and impls may name a borrowing type declared further down, so names come first.
    let mut names = TypeNames::new(|attrs, ident, generics| {
        matches!(marker(attrs, ident, generics), Ok(Some(_)))
    });
    names.visit_file_mut(file);
    let mut pass = Lower {
        types: names.found,
        errors: None,
    };
    pass.visit_file_mut(file);
    pass.errors.map_or(Ok(()), Err)
}

/// `'__rustx_borrows`: resolved like macro output, so it cannot clash with a user lifetime, and
/// reported at `user`.
pub(super) fn lifetime(user: Span) -> Lifetime {
    let span = introduced_span(user);
    Lifetime {
        apostrophe: span,
        ident: Ident::new(NAME, span),
    }
}

/// Whether `generics` declare the lifetime this pass adds, i.e. belong to a borrowing type or
/// to an `impl` this pass extended.
pub(super) fn has_lifetime(generics: &Generics) -> bool {
    generics
        .lifetimes()
        .any(|param| param.lifetime.ident == NAME)
}

/// Names of the borrowing types in `file`, once this pass ran.
pub(super) fn borrowing_types(file: &mut syn::File) -> HashSet<String> {
    let mut names = TypeNames::new(|_, _, generics| has_lifetime(generics));
    names.visit_file_mut(file);
    names.found
}

/// Collects the names of the structs and enums `pick` accepts, at any depth.
struct TypeNames<F> {
    pick: F,
    found: HashSet<String>,
}

impl<F: Fn(&[Attribute], &Ident, &Generics) -> bool> TypeNames<F> {
    fn new(pick: F) -> Self {
        TypeNames {
            pick,
            found: HashSet::new(),
        }
    }

    fn check(&mut self, attrs: &[Attribute], ident: &Ident, generics: &Generics) {
        if (self.pick)(attrs, ident, generics) {
            self.found.insert(ident.to_string());
        }
    }
}

impl<F: Fn(&[Attribute], &Ident, &Generics) -> bool> VisitMut for TypeNames<F> {
    fn visit_item_struct_mut(&mut self, item: &mut ItemStruct) {
        self.check(&item.attrs, &item.ident, &item.generics);
    }

    fn visit_item_enum_mut(&mut self, item: &mut ItemEnum) {
        self.check(&item.attrs, &item.ident, &item.generics);
        visit_mut::visit_item_enum_mut(self, item);
    }
}

fn is_borrows(attr: &Attribute) -> bool {
    matches!(attr.style, AttrStyle::Outer) && attr.path().is_ident("borrows")
}

/// The span of `#[borrows]` on a type, or the reason the type cannot take it.
fn marker(attrs: &[Attribute], ident: &Ident, generics: &Generics) -> syn::Result<Option<Span>> {
    let Some(attr) = attrs.iter().find(|attr| is_borrows(attr)) else {
        return Ok(None);
    };
    if !matches!(attr.meta, Meta::Path(_)) {
        let msg = "`#[borrows]` takes no arguments";
        return Err(syn::Error::new_spanned(&attr.meta, msg));
    }
    if let Some(param) = generics.lifetimes().next() {
        let msg = format!(
            "`#[borrows]` adds exactly one lifetime, and `{ident}` already declares `{}`; \
             a type with several lifetimes keeps explicit parameters",
            param.lifetime
        );
        return Err(syn::Error::new_spanned(&param.lifetime, msg));
    }
    Ok(Some(attr.path().span()))
}

struct Lower {
    types: HashSet<String>,
    errors: Option<syn::Error>,
}

impl Lower {
    fn lower_type<'f>(
        &mut self,
        attrs: &mut Vec<Attribute>,
        ident: &Ident,
        generics: &mut Generics,
        fields: impl Iterator<Item = &'f mut Field>,
    ) {
        let marked = marker(attrs, ident, generics);
        attrs.retain(|attr| !is_borrows(attr));
        let span = match marked {
            Ok(Some(span)) => span,
            Ok(None) => return,
            Err(err) => return combine(&mut self.errors, err),
        };
        let lifetime = lifetime(span);
        let mut fill = Fill::new(&lifetime, &self.types, true);
        for field in fields {
            fill.visit_type_mut(&mut field.ty);
        }
        if fill.count == 0 {
            let msg = format!(
                "`{ident}` holds no reference without a lifetime, so `#[borrows]` has nothing to add"
            );
            return combine(&mut self.errors, syn::Error::new(span, msg));
        }
        generics
            .params
            .insert(0, GenericParam::Lifetime(LifetimeParam::new(lifetime)));
    }

    fn lower_impl(&self, item: &mut ItemImpl) {
        let lifetime = lifetime(item.impl_token.span);
        let mut fill = Fill::new(&lifetime, &self.types, false);
        fill.visit_type_mut(&mut item.self_ty);
        if let Some((path, _)) = &mut item.trait_ {
            fill.visit_path_mut(path);
        }
        if fill.count == 0 {
            return;
        }
        fill.visit_generics_mut(&mut item.generics);
        for impl_item in &mut item.items {
            match impl_item {
                ImplItem::Fn(func) => fill.visit_signature_mut(&mut func.sig),
                ImplItem::Type(ty) => fill.visit_type_mut(&mut ty.ty),
                ImplItem::Const(constant) => fill.visit_type_mut(&mut constant.ty),
                _ => {}
            }
        }
        item.generics
            .params
            .insert(0, GenericParam::Lifetime(LifetimeParam::new(lifetime)));
    }
}

impl VisitMut for Lower {
    fn visit_item_struct_mut(&mut self, item: &mut ItemStruct) {
        let ItemStruct {
            attrs,
            ident,
            generics,
            fields,
            ..
        } = item;
        self.lower_type(attrs, ident, generics, fields.iter_mut());
    }

    fn visit_item_enum_mut(&mut self, item: &mut ItemEnum) {
        let ItemEnum {
            attrs,
            ident,
            generics,
            variants,
            ..
        } = item;
        let fields = variants
            .iter_mut()
            .flat_map(|variant| variant.fields.iter_mut());
        self.lower_type(attrs, ident, generics, fields);
        // Discriminant expressions may hold blocks with items of their own.
        visit_mut::visit_item_enum_mut(self, item);
    }

    fn visit_item_impl_mut(&mut self, item: &mut ItemImpl) {
        self.lower_impl(item);
        visit_mut::visit_item_impl_mut(self, item);
    }
}

/// Puts the lifetime on borrowing types written without a lifetime argument and, with
/// `references`, on elided references and `'_`.
pub(super) struct Fill<'a> {
    lifetime: &'a Lifetime,
    types: &'a HashSet<String>,
    references: bool,
    /// How many places took the lifetime.
    pub(super) count: usize,
}

impl<'a> Fill<'a> {
    pub(super) fn new(
        lifetime: &'a Lifetime,
        types: &'a HashSet<String>,
        references: bool,
    ) -> Self {
        Fill {
            lifetime,
            types,
            references,
            count: 0,
        }
    }
}

impl VisitMut for Fill<'_> {
    fn visit_type_reference_mut(&mut self, reference: &mut TypeReference) {
        if self.references && reference.lifetime.is_none() {
            reference.lifetime = Some(self.lifetime.clone());
            self.count += 1;
        }
        visit_mut::visit_type_reference_mut(self, reference);
    }

    fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
        if self.references && lifetime.ident == "_" {
            *lifetime = self.lifetime.clone();
            self.count += 1;
        }
    }

    fn visit_type_path_mut(&mut self, ty: &mut TypePath) {
        if ty.qself.is_none()
            && let Some(last) = ty.path.segments.last_mut()
            && self.types.contains(&last.ident.to_string())
            && add_lifetime(&mut last.arguments, self.lifetime)
        {
            self.count += 1;
        }
        visit_mut::visit_type_path_mut(self, ty);
    }

    // `fn(&T) -> &U` and `Fn(&T) -> &U` elide within their own signature, not the type's.
    fn visit_type_fn_ptr_mut(&mut self, _: &mut TypeFnPtr) {}

    fn visit_parenthesized_generic_arguments_mut(&mut self, _: &mut ParenthesizedGenericArguments) {
    }
}

/// Adds the lifetime first to arguments that have none; `false` when they already name one.
fn add_lifetime(arguments: &mut PathArguments, lifetime: &Lifetime) -> bool {
    let argument = GenericArgument::Lifetime(lifetime.clone());
    match arguments {
        PathArguments::None => {
            *arguments = PathArguments::AngleBracketed(AngleBracketedGenericArguments {
                colon2_token: None,
                lt_token: Token![<](lifetime.apostrophe),
                args: Punctuated::from_iter([argument]),
                gt_token: Token![>](lifetime.apostrophe),
            });
            true
        }
        PathArguments::AngleBracketed(angle)
            if !angle
                .args
                .iter()
                .any(|arg| matches!(arg, GenericArgument::Lifetime(_))) =>
        {
            angle.args.insert(0, argument);
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::{ToTokens, quote};

    fn lowered(source: proc_macro2::TokenStream) -> syn::Result<String> {
        let mut file: syn::File = syn::parse2(source)?;
        lower(&mut file)?;
        Ok(file.into_token_stream().to_string())
    }

    fn printed(plain: proc_macro2::TokenStream) -> String {
        syn::parse2::<syn::File>(plain)
            .unwrap()
            .into_token_stream()
            .to_string()
    }

    fn errors(source: proc_macro2::TokenStream) -> Vec<String> {
        lowered(source)
            .unwrap_err()
            .into_iter()
            .map(|err| err.to_string())
            .collect()
    }

    #[test]
    fn borrows_adds_one_lifetime_to_the_type_its_fields_and_its_impls() {
        assert_eq!(
            lowered(quote! {
                #[borrows]
                #[derive(Debug)]
                pub struct Lexer { src: &str, pos: usize }
                impl Lexer { fn rest(&self) -> &str { self.src } }
                impl Iterator for Lexer { type Item = char; fn next(&mut self) -> Option<char> { None } }
            })
            .unwrap(),
            printed(quote! {
                #[derive(Debug)]
                pub struct Lexer<'__rustx_borrows> { src: &'__rustx_borrows str, pos: usize }
                impl<'__rustx_borrows> Lexer<'__rustx_borrows> { fn rest(&self) -> &str { self.src } }
                impl<'__rustx_borrows> Iterator for Lexer<'__rustx_borrows> {
                    type Item = char;
                    fn next(&mut self) -> Option<char> { None }
                }
            })
        );
    }

    #[test]
    fn every_elided_lifetime_in_fields_takes_the_lifetime_except_in_fn_types() {
        assert_eq!(
            lowered(quote! {
                #[borrows]
                enum E<T> { A(&Vec<&T>), B { it: Iter<'_, u8>, f: fn(&u8) -> &u8, s: &'static str } }
            })
            .unwrap(),
            printed(quote! {
                enum E<'__rustx_borrows, T> {
                    A(&'__rustx_borrows Vec<&'__rustx_borrows T>),
                    B { it: Iter<'__rustx_borrows, u8>, f: fn(&u8) -> &u8, s: &'static str }
                }
            })
        );
    }

    #[test]
    fn borrowing_types_in_fields_and_impl_signatures_take_the_lifetime() {
        assert_eq!(
            lowered(quote! {
                struct Parser { tokens: Peekable<Lexer>, last: Option<lex::Token> }
                #[borrows] struct Lexer<T> { src: &T }
                #[borrows] enum Token { Ident(&str) }
                #[borrows] struct Parser2 { tokens: Peekable<Lexer<u8>> }
                impl<T> Lexer<T> {
                    const NONE: Option<Token> = None;
                    fn new(src: &T) -> Lexer<T> { Lexer { src } }
                    fn next(&mut self, f: impl Fn(Token)) -> Option<Token> { let t: Token = todo!(); }
                }
                impl Iterator for Parser2 { type Item = Token; }
                impl PartialEq<Token> for Wrapper {}
            })
            .unwrap(),
            printed(quote! {
                struct Parser { tokens: Peekable<Lexer>, last: Option<lex::Token> }
                struct Lexer<'__rustx_borrows, T> { src: &'__rustx_borrows T }
                enum Token<'__rustx_borrows> { Ident(&'__rustx_borrows str) }
                struct Parser2<'__rustx_borrows> {
                    tokens: Peekable<Lexer<'__rustx_borrows, u8>>
                }
                impl<'__rustx_borrows, T> Lexer<'__rustx_borrows, T> {
                    const NONE: Option<Token<'__rustx_borrows>> = None;
                    fn new(src: &T) -> Lexer<'__rustx_borrows, T> { Lexer { src } }
                    fn next(&mut self, f: impl Fn(Token)) -> Option<Token<'__rustx_borrows>> {
                        let t: Token = todo!();
                    }
                }
                impl<'__rustx_borrows> Iterator for Parser2<'__rustx_borrows> {
                    type Item = Token<'__rustx_borrows>;
                }
                impl<'__rustx_borrows> PartialEq<Token<'__rustx_borrows>> for Wrapper {}
            })
        );
    }

    #[test]
    fn impls_with_written_lifetimes_and_of_other_types_are_unchanged() {
        let plain = quote! {
            impl<'a> Lexer<'a> { fn f(&self) -> Token { todo!() } }
            impl Other { fn f(&self) -> Token { todo!() } }
        };
        let sugar =
            quote! { #[borrows] struct Lexer { src: &str } #[borrows] enum Token { A(&str) } };
        let expanded = lowered(quote! { #sugar #plain }).unwrap();
        assert!(expanded.ends_with(&printed(plain)), "{expanded}");
    }

    #[test]
    fn valid_rust_is_unchanged() {
        let plain = quote! {
            #[derive(Debug)]
            struct S<'a> { a: &'a str }
            impl<'a> S<'a> { fn f(&self) -> &str { self.a } }
            #[other] struct T { n: u8 }
            mod m { fn g(x: &u8) -> &u8 { x } }
        };
        assert_eq!(lowered(plain.clone()).unwrap(), printed(plain));
    }

    #[test]
    fn a_type_without_references_is_an_error() {
        assert_eq!(
            errors(quote! { #[borrows] struct S { n: usize, f: fn(&u8) } }),
            ["`S` holds no reference without a lifetime, so `#[borrows]` has nothing to add"]
        );
    }

    #[test]
    fn a_type_that_already_has_a_lifetime_is_an_error() {
        let found = errors(quote! { #[borrows] struct S<'a> { a: &'a str, b: &str } });
        assert_eq!(found.len(), 1);
        assert!(
            found[0].starts_with(
                "`#[borrows]` adds exactly one lifetime, and `S` already declares `'a`"
            )
        );
    }

    #[test]
    fn arguments_are_an_error_and_errors_are_combined() {
        assert_eq!(
            errors(quote! {
                #[borrows(x)] struct A { a: &str }
                #[borrows] enum B { N(u8) }
            }),
            [
                "`#[borrows]` takes no arguments",
                "`B` holds no reference without a lifetime, so `#[borrows]` has nothing to add",
            ]
        );
    }
}
