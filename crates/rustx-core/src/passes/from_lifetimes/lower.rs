//! AST half of `&from(..)` lifetimes: resolves the marker lifetimes the token pass left.
//!
//! Per signature, the markers of the return type name parameters. Parameters named together in
//! one `from(..)`, or linked through several, share one lifetime `'__rustx_from_<first>`, added
//! after the signature's own lifetime parameters. Every named parameter gets that lifetime on
//! its elided lifetimes:
//!
//! - an elided `&` / `&mut` (also `&self`), the outermost one only: in `&Vec<&str>` the inner
//!   reference keeps its own lifetime, because the returned borrow comes from the outer one;
//! - `'_` arguments (`Iter<'_, T>`);
//! - inside `impl Trait` and inside the bounds of a bare type parameter (`it: I` with
//!   `I: Iterator<Item = &Entry>`). Both also gain `+ 'lt`: the result borrows through the
//!   parameter, so the parameter must live that long.
//!
//! A parameter with none of these cannot lend anything, and `from(..)` on it is an error.
//!
//! A `'_` bound of an `impl Trait` in the return type (what `iter T` expands to) becomes the
//! `from` lifetime used inside that `impl Trait`: `iter &from(a) T` borrows from `a`, not from
//! whatever `'_` would elide to. With markers of two different lifetimes inside, that `'_` is
//! ambiguous and reported; `from(a, b)` names both sources with one lifetime.
//!
//! # Limitations
//!
//! - Only parameters bound to a plain name (`x: T`, `mut x: T`, `self`) can be named; a
//!   destructuring pattern has no name to put in `from(..)`.
//! - `from(Self)` and `from(Type)` belong to `#[borrows]` (problem 19); here they name no
//!   parameter and are reported as such.
//! - A parameter whose references all carry written lifetimes (`&'a T`) cannot be named.

use std::collections::HashMap;

use proc_macro2::{Ident, Span};
use syn::ext::IdentExt;
use syn::punctuated::Punctuated;
use syn::visit_mut::{self, VisitMut};
use syn::{
    FnArg, GenericParam, Generics, Lifetime, LifetimeParam, ParenthesizedGenericArguments, Pat,
    ReceiverKind, Signature, Token, Type, TypeFnPtr, TypeImplTrait, TypeParamBound, TypeReference,
    WherePredicate,
};

use super::{decode, introduced_span};

pub(in crate::passes) fn lower(file: &mut syn::File) -> syn::Result<()> {
    let mut pass = Lower { errors: None };
    pass.visit_file_mut(file);
    pass.errors.map_or(Ok(()), Err)
}

fn push(errors: &mut Option<syn::Error>, err: syn::Error) {
    match errors {
        Some(all) => all.combine(err),
        None => *errors = Some(err),
    }
}

struct Lower {
    errors: Option<syn::Error>,
}

impl VisitMut for Lower {
    fn visit_signature_mut(&mut self, sig: &mut Signature) {
        if let Err(err) = lower_signature(sig) {
            push(&mut self.errors, err);
        }
        // The return type is resolved or already reported; a marker anywhere else is misplaced.
        self.visit_generics_mut(&mut sig.generics);
        for arg in &mut sig.inputs {
            self.visit_fn_arg_mut(arg);
        }
    }

    fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
        if decode(&lifetime.ident).is_some() {
            let msg = "`&from(..)` only works in the return type of a function";
            push(&mut self.errors, syn::Error::new_spanned(&*lifetime, msg));
        }
    }
}

/// One `from(..)` as the token pass encoded it.
struct Marker {
    key: String,
    names: Vec<String>,
    /// The marker as written, spanned over the user's `from(..)`.
    written: Lifetime,
}

#[derive(Default)]
struct Collect(Vec<Marker>);

impl VisitMut for Collect {
    fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
        if let Some(names) = decode(&lifetime.ident) {
            self.0.push(Marker {
                key: lifetime.ident.to_string(),
                names,
                written: lifetime.clone(),
            });
        }
    }
}

fn lower_signature(sig: &mut Signature) -> syn::Result<()> {
    let mut collect = Collect::default();
    collect.visit_return_type_mut(&mut sig.output);
    let markers = collect.0;
    if markers.is_empty() {
        return Ok(());
    }
    let params: Vec<Option<String>> = sig.inputs.iter().map(param_name).collect();
    let mut errors = None;
    // Union-find over parameter indices: names in one `from(..)` share a lifetime.
    let mut parent: Vec<usize> = (0..params.len()).collect();
    // The marker that first named each parameter, for its spans.
    let mut named_by: Vec<Option<&Marker>> = vec![None; params.len()];
    for marker in &markers {
        let mut anchor = None;
        for name in &marker.names {
            let Some(index) = params.iter().position(|p| p.as_deref() == Some(name)) else {
                let msg = format!("`from(..)` names `{name}`, which is not a parameter here");
                push(&mut errors, syn::Error::new_spanned(&marker.written, msg));
                continue;
            };
            named_by[index].get_or_insert(marker);
            match anchor {
                Some(first) => {
                    let (a, b) = (root(&parent, first), root(&parent, index));
                    parent[b] = a;
                }
                None => anchor = Some(index),
            }
        }
    }
    if let Some(err) = errors {
        return Err(err);
    }

    // One lifetime per class, named after its first parameter, in parameter order.
    let mut by_root: HashMap<usize, Lifetime> = HashMap::new();
    let mut introduced = Vec::new();
    for (index, marker) in named_by.iter().enumerate() {
        let (Some(marker), Some(name)) = (marker, &params[index]) else {
            continue;
        };
        let lifetime = by_root
            .entry(root(&parent, index))
            .or_insert_with(|| {
                let lifetime = fresh(name, marker.written.apostrophe);
                introduced.push(lifetime.clone());
                lifetime
            })
            .clone();
        if !tie(sig, index, &lifetime) {
            let msg = format!("`{name}` is not a reference, so `from(..)` cannot borrow from it");
            push(&mut errors, syn::Error::new_spanned(&marker.written, msg));
        }
    }
    if let Some(err) = errors {
        return Err(err);
    }

    let at = sig
        .generics
        .params
        .iter()
        .take_while(|param| matches!(param, GenericParam::Lifetime(_)))
        .count();
    for (offset, lifetime) in introduced.into_iter().enumerate() {
        let param = GenericParam::Lifetime(LifetimeParam::new(lifetime));
        sig.generics.params.insert(at + offset, param);
    }

    let mut resolve = Resolve {
        by_key: HashMap::new(),
        errors: None,
    };
    for marker in &markers {
        let index = marker
            .names
            .first()
            .and_then(|name| params.iter().position(|p| p.as_deref() == Some(name)));
        if let Some(lifetime) = index.and_then(|i| by_root.get(&root(&parent, i))) {
            resolve.by_key.insert(marker.key.clone(), lifetime.clone());
        }
    }
    resolve.visit_return_type_mut(&mut sig.output);
    resolve.errors.map_or(Ok(()), Err)
}

fn root(parent: &[usize], mut index: usize) -> usize {
    while parent[index] != index {
        index = parent[index];
    }
    index
}

fn param_name(arg: &FnArg) -> Option<String> {
    match arg {
        FnArg::Receiver(_) => Some("self".to_owned()),
        FnArg::Typed(typed) => match &*typed.pat {
            Pat::Ident(pat) if pat.subpat.is_none() => Some(pat.ident.unraw().to_string()),
            _ => None,
        },
    }
}

fn fresh(param: &str, from: Span) -> Lifetime {
    let span = introduced_span(from);
    Lifetime {
        apostrophe: span,
        ident: Ident::new(&format!("__rustx_from_{param}"), span),
    }
}

/// Gives parameter `index` the lifetime; `false` when it has no elided lifetime to take it.
fn tie(sig: &mut Signature, index: usize, lifetime: &Lifetime) -> bool {
    let Signature {
        generics, inputs, ..
    } = sig;
    let Some(arg) = inputs.iter_mut().nth(index) else {
        return false;
    };
    let ty = match arg {
        FnArg::Receiver(receiver) => match &mut receiver.kind {
            ReceiverKind::Reference(_, slot, _) => return fill_slot(slot, lifetime),
            ReceiverKind::Typed(_, ty) => &mut **ty,
            _ => return false,
        },
        FnArg::Typed(typed) => &mut *typed.ty,
    };
    if let Type::Path(path) = ty
        && path.qself.is_none()
        && let Some(ident) = path.path.get_ident()
        && generics.type_params().any(|param| param.ident == *ident)
    {
        return tie_type_param(generics, ident, lifetime);
    }
    match ty {
        Type::ImplTrait(imp) => {
            let filled = fill_bounds(&mut imp.bounds, lifetime);
            if filled {
                imp.bounds.push(TypeParamBound::Lifetime(lifetime.clone()));
            }
            filled
        }
        ty => {
            let mut elided = Elided::new(lifetime);
            elided.visit_type_mut(ty);
            elided.count > 0
        }
    }
}

/// Fills the bounds of type parameter `ident`, inline and in the `where` clause.
fn tie_type_param(generics: &mut Generics, ident: &Ident, lifetime: &Lifetime) -> bool {
    let mut filled = false;
    if let Some(clause) = &mut generics.where_clause {
        for predicate in &mut clause.predicates {
            if let WherePredicate::Type(predicate) = predicate
                && matches!(&predicate.bounded_ty, Type::Path(p) if p.qself.is_none() && p.path.is_ident(ident))
            {
                filled |= fill_bounds(&mut predicate.bounds, lifetime);
            }
        }
    }
    let Some(param) = generics
        .type_params_mut()
        .find(|param| param.ident == *ident)
    else {
        return false;
    };
    filled |= fill_bounds(&mut param.bounds, lifetime);
    if filled {
        param
            .colon_token
            .get_or_insert(Token![:](lifetime.apostrophe));
        param
            .bounds
            .push(TypeParamBound::Lifetime(lifetime.clone()));
    }
    filled
}

fn fill_bounds(bounds: &mut Punctuated<TypeParamBound, Token![+]>, lifetime: &Lifetime) -> bool {
    let mut elided = Elided::new(lifetime);
    for bound in bounds.iter_mut() {
        elided.visit_type_param_bound_mut(bound);
    }
    elided.count > 0
}

fn fill_slot(slot: &mut Option<Lifetime>, lifetime: &Lifetime) -> bool {
    if slot.as_ref().is_some_and(|written| written.ident != "_") {
        return false;
    }
    *slot = Some(lifetime.clone());
    true
}

/// Puts the lifetime on the outermost elided lifetimes of a type.
struct Elided<'a> {
    lifetime: &'a Lifetime,
    count: usize,
}

impl<'a> Elided<'a> {
    fn new(lifetime: &'a Lifetime) -> Self {
        Elided { lifetime, count: 0 }
    }
}

impl VisitMut for Elided<'_> {
    fn visit_type_reference_mut(&mut self, reference: &mut TypeReference) {
        if fill_slot(&mut reference.lifetime, self.lifetime) {
            self.count += 1;
        } else {
            visit_mut::visit_type_reference_mut(self, reference);
        }
    }

    fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
        if lifetime.ident == "_" {
            *lifetime = self.lifetime.clone();
            self.count += 1;
        }
    }

    // `fn(&T) -> &U` and `Fn(&T) -> &U` elide within their own signature, not the outer one.
    fn visit_type_fn_ptr_mut(&mut self, _: &mut TypeFnPtr) {}

    fn visit_parenthesized_generic_arguments_mut(&mut self, _: &mut ParenthesizedGenericArguments) {
    }
}

/// Replaces the markers of a return type with their lifetimes.
struct Resolve {
    by_key: HashMap<String, Lifetime>,
    errors: Option<syn::Error>,
}

impl VisitMut for Resolve {
    fn visit_type_impl_trait_mut(&mut self, imp: &mut TypeImplTrait) {
        let mut collect = Collect::default();
        collect.visit_type_impl_trait_mut(imp);
        let mut used: Vec<&Lifetime> = Vec::new();
        for marker in &collect.0 {
            if let Some(lifetime) = self.by_key.get(&marker.key)
                && !used.iter().any(|seen| seen.ident == lifetime.ident)
            {
                used.push(lifetime);
            }
        }
        for bound in &mut imp.bounds {
            let TypeParamBound::Lifetime(lifetime) = bound else {
                continue;
            };
            if lifetime.ident != "_" {
                continue;
            }
            match used.as_slice() {
                [] => {}
                [only] => *lifetime = (*only).clone(),
                _ => {
                    let msg = "`'_` would borrow from several `from(..)` sources here; \
                               name them in one `from(a, b)`";
                    push(&mut self.errors, syn::Error::new(lifetime.apostrophe, msg));
                }
            }
        }
        visit_mut::visit_type_impl_trait_mut(self, imp);
    }

    fn visit_lifetime_mut(&mut self, lifetime: &mut Lifetime) {
        if let Some(resolved) = self.by_key.get(&lifetime.ident.to_string()) {
            *lifetime = resolved.clone();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::{ToTokens, quote};

    /// Runs both halves, like `try_expand`, without the other passes.
    fn lowered(sugar: proc_macro2::TokenStream) -> syn::Result<String> {
        let tokens = super::super::rewrite(sugar)?;
        let mut file: syn::File = syn::parse2(tokens)?;
        lower(&mut file)?;
        Ok(file.into_token_stream().to_string())
    }

    fn printed(plain: proc_macro2::TokenStream) -> String {
        syn::parse2::<syn::File>(plain)
            .unwrap()
            .into_token_stream()
            .to_string()
    }

    fn error(sugar: proc_macro2::TokenStream) -> String {
        lowered(sugar).unwrap_err().to_string()
    }

    #[test]
    fn from_adds_a_lifetime_to_generics_parameter_and_return() {
        assert_eq!(
            lowered(quote! { fn first(xs: &[Item], key: &str) -> &from(xs) Item { &xs[0] } })
                .unwrap(),
            printed(quote! {
                fn first<'__rustx_from_xs>(xs: &'__rustx_from_xs [Item], key: &str)
                    -> &'__rustx_from_xs Item { &xs[0] }
            })
        );
    }

    #[test]
    fn from_self_ties_the_receiver() {
        assert_eq!(
            lowered(quote! { impl C { fn get(&self, k: &str) -> Option<&from(self) str> {} } })
                .unwrap(),
            printed(quote! { impl C {
                fn get<'__rustx_from_self>(&'__rustx_from_self self, k: &str)
                    -> Option<&'__rustx_from_self str> {}
            } })
        );
    }

    #[test]
    fn several_names_share_one_lifetime_after_existing_lifetimes() {
        assert_eq!(
            lowered(quote! {
                fn f<'x, T>(a: &mut T, b: Iter<'_, u8>, c: &'x u8) -> (&from(a, b) T, &from(b) u8) {}
            })
            .unwrap(),
            printed(quote! {
                fn f<'x, '__rustx_from_a, T>(a: &'__rustx_from_a mut T, b: Iter<'__rustx_from_a, u8>, c: &'x u8)
                    -> (&'__rustx_from_a T, &'__rustx_from_a u8) {}
            })
        );
    }

    #[test]
    fn only_the_outer_reference_of_a_parameter_is_tied() {
        assert_eq!(
            lowered(quote! { fn f(v: &Vec<&str>, g: fn(&u8) -> &u8) -> &from(v) str {} }).unwrap(),
            printed(quote! {
                fn f<'__rustx_from_v>(v: &'__rustx_from_v Vec<&str>, g: fn(&u8) -> &u8)
                    -> &'__rustx_from_v str {}
            })
        );
    }

    #[test]
    fn iter_elided_bound_becomes_the_from_lifetime() {
        // What `prefix_types` makes of `iter &from(self, needle) Entry`.
        assert_eq!(
            lowered(quote! {
                fn grep(&self, needle: &str) -> impl Iterator<Item = &from(self, needle) Entry> + '_ {}
            })
            .unwrap(),
            printed(quote! {
                fn grep<'__rustx_from_self>(&'__rustx_from_self self, needle: &'__rustx_from_self str)
                    -> impl Iterator<Item = &'__rustx_from_self Entry> + '__rustx_from_self {}
            })
        );
    }

    #[test]
    fn type_parameter_bounds_take_the_lifetime_and_outlive_it() {
        assert_eq!(
            lowered(quote! {
                fn numbered<I: Iterator<Item = &Entry>>(it: I) -> impl Iterator<Item = (usize, &from(it) Entry)> + '_ {}
                fn g<J>(it: J, s: impl AsRef<[&u8]>) -> (&from(it) u8, &from(s) u8) where J: Iterator<Item = &u8> {}
            })
            .unwrap(),
            printed(quote! {
                fn numbered<'__rustx_from_it, I: Iterator<Item = &'__rustx_from_it Entry> + '__rustx_from_it>(it: I)
                    -> impl Iterator<Item = (usize, &'__rustx_from_it Entry)> + '__rustx_from_it {}
                fn g<'__rustx_from_it, '__rustx_from_s, J: '__rustx_from_it>(it: J, s: impl AsRef<[&'__rustx_from_s u8]> + '__rustx_from_s)
                    -> (&'__rustx_from_it u8, &'__rustx_from_s u8) where J: Iterator<Item = &'__rustx_from_it u8> {}
            })
        );
    }

    #[test]
    fn functions_without_from_are_unchanged() {
        let plain = quote! {
            fn f(x: &u8) -> impl Iterator<Item = &u8> + '_ { None.into_iter() }
            impl S { fn g<'a>(&'a self) -> &'a u8 { &self.0 } }
        };
        assert_eq!(lowered(plain.clone()).unwrap(), printed(plain));
    }

    #[test]
    fn unknown_name_is_an_error() {
        assert_eq!(
            error(quote! { fn f(x: &u8) -> &from(y) u8 {} }),
            "`from(..)` names `y`, which is not a parameter here"
        );
    }

    #[test]
    fn non_reference_parameter_is_an_error() {
        for sugar in [
            quote! { fn f(x: u8) -> &from(x) u8 {} },
            quote! { fn f(x: &'static u8) -> &from(x) u8 {} },
            quote! { impl S { fn f(self) -> &from(self) u8 {} } },
            quote! { fn f<I: Iterator<Item = u8>>(x: I) -> &from(x) u8 {} },
        ] {
            assert!(error(sugar).contains("is not a reference"));
        }
    }

    #[test]
    fn from_outside_a_return_type_is_an_error() {
        assert_eq!(
            error(quote! { fn f(x: &from(x) u8) {} }),
            "`&from(..)` only works in the return type of a function"
        );
        assert!(error(quote! { struct S { f: &from(x) u8 } }).contains("only works"));
    }

    #[test]
    fn elided_bound_over_two_sources_is_an_error() {
        assert!(
            error(quote! {
                fn f(a: &u8, b: &u8) -> impl Iterator<Item = (&from(a) u8, &from(b) u8)> + '_ {}
            })
            .contains("several `from(..)` sources")
        );
    }

    #[test]
    fn errors_of_several_functions_are_combined() {
        let err = lowered(quote! {
            fn f(x: &u8) -> &from(y) u8 {}
            fn g(x: u8) -> &from(x) u8 {}
        })
        .unwrap_err();
        assert_eq!(err.into_iter().count(), 2);
    }
}
