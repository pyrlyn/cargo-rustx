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
//! # Borrowing types (problem 19)
//!
//! `from(Self)` and `from(Type)`, where `Type` is a `#[borrows]` type, name that type's lifetime,
//! in parameters as well as in the return type. The `borrows` pass ran first and left the lifetime
//! in the generics of each borrowing type and of each `impl` it extended, which is how this pass
//! recognises both:
//!
//! - inside such an `impl` the lifetime is in scope, so both forms resolve to it;
//! - elsewhere `from(Self)` is an error, and `from(Type)` adds the lifetime to the function and
//!   to every parameter type `Type` written without a lifetime argument.
//!
//! # Limitations
//!
//! - Only parameters bound to a plain name (`x: T`, `mut x: T`, `self`) can be named; a
//!   destructuring pattern has no name to put in `from(..)`.
//! - `Self` and the names of borrowing types always name types, never parameters, and take the
//!   whole `from(..)`: one lifetime cannot be both a type's and a parameter's.
//! - A parameter whose references all carry written lifetimes (`&'a T`) cannot be named.

use std::collections::{HashMap, HashSet};

use proc_macro2::{Ident, Span};
use syn::ext::IdentExt;
use syn::punctuated::Punctuated;
use syn::visit_mut::{self, VisitMut};
use syn::{
    FnArg, GenericParam, Generics, ItemFn, ItemImpl, ItemTrait, Lifetime, LifetimeParam,
    ParenthesizedGenericArguments, Pat, ReceiverKind, Signature, Token, Type, TypeFnPtr,
    TypeImplTrait, TypeParamBound, TypeReference, WherePredicate,
};

use super::{decode, introduced_span};
use crate::passes::borrows;
use crate::tokens::syntax::{bare_name, combine};

pub(in crate::passes) fn lower(file: &mut syn::File) -> syn::Result<()> {
    let mut pass = Lower {
        types: borrows::borrowing_types(file),
        in_borrowing_impl: false,
        errors: None,
    };
    pass.visit_file_mut(file);
    pass.errors.map_or(Ok(()), Err)
}

struct Lower {
    /// Names of the `#[borrows]` types.
    types: HashSet<String>,
    /// Whether the signatures visited now are in an `impl` that has the `#[borrows]` lifetime.
    in_borrowing_impl: bool,
    errors: Option<syn::Error>,
}

impl Lower {
    fn scoped(&mut self, in_borrowing_impl: bool, visit: impl FnOnce(&mut Self)) {
        let outer = std::mem::replace(&mut self.in_borrowing_impl, in_borrowing_impl);
        visit(self);
        self.in_borrowing_impl = outer;
    }
}

impl VisitMut for Lower {
    fn visit_item_impl_mut(&mut self, item: &mut ItemImpl) {
        let inside = borrows::has_lifetime(&item.generics);
        self.scoped(inside, |pass| visit_mut::visit_item_impl_mut(pass, item));
    }

    // Items nested in a method body do not see the generics of the `impl` around it.
    fn visit_item_fn_mut(&mut self, item: &mut ItemFn) {
        self.scoped(false, |pass| visit_mut::visit_item_fn_mut(pass, item));
    }

    fn visit_item_trait_mut(&mut self, item: &mut ItemTrait) {
        self.scoped(false, |pass| visit_mut::visit_item_trait_mut(pass, item));
    }

    fn visit_signature_mut(&mut self, sig: &mut Signature) {
        if let Err(err) = lower_type_markers(sig, &self.types, self.in_borrowing_impl) {
            combine(&mut self.errors, err);
        }
        if let Err(err) = lower_signature(sig) {
            combine(&mut self.errors, err);
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
            combine(&mut self.errors, syn::Error::new_spanned(&*lifetime, msg));
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
                combine(&mut errors, syn::Error::new_spanned(&marker.written, msg));
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
            combine(&mut errors, syn::Error::new_spanned(&marker.written, msg));
        }
    }
    if let Some(err) = errors {
        return Err(err);
    }

    let at = lifetimes_end(&sig.generics);
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

/// Resolves `from(Self)` and `from(Type)` anywhere in the signature to the `#[borrows]` lifetime,
/// before [`lower_signature`] looks for parameters.
fn lower_type_markers(
    sig: &mut Signature,
    types: &HashSet<String>,
    in_borrowing_impl: bool,
) -> syn::Result<()> {
    let mut collect = Collect::default();
    for arg in &mut sig.inputs {
        collect.visit_fn_arg_mut(arg);
    }
    collect.visit_return_type_mut(&mut sig.output);
    let mut errors = None;
    let mut resolve = Resolve {
        by_key: HashMap::new(),
        errors: None,
    };
    // Types named outside a borrowing `impl`, each with the marker that first named it.
    let mut named: Vec<(&str, &Marker)> = Vec::new();
    for marker in &collect.0 {
        let Some(name) = marker
            .names
            .iter()
            .find(|name| *name == "Self" || types.contains(*name))
        else {
            continue;
        };
        if marker.names.len() > 1 {
            let msg = format!("`from({name})` names a type's lifetime, so it takes no other names");
            combine(&mut errors, syn::Error::new_spanned(&marker.written, msg));
        } else if in_borrowing_impl {
            // The `impl` already declares the lifetime and gave it to every borrowing type.
        } else if name == "Self" {
            let msg = "`from(Self)` only works in an `impl` of a `#[borrows]` type";
            combine(&mut errors, syn::Error::new_spanned(&marker.written, msg));
        } else if !named.iter().any(|(seen, _)| seen == name) {
            named.push((name, marker));
        }
        // Resolved even when reported, so the parameter checks do not report it again.
        let lifetime = borrows::lifetime(marker.written.apostrophe);
        resolve.by_key.insert(marker.key.clone(), lifetime);
    }
    if let Some((_, first)) = named.first() {
        let lifetime = borrows::lifetime(first.written.apostrophe);
        let param = GenericParam::Lifetime(LifetimeParam::new(lifetime.clone()));
        sig.generics
            .params
            .insert(lifetimes_end(&sig.generics), param);
        for (name, marker) in &named {
            let only = HashSet::from([(*name).to_owned()]);
            let mut fill = borrows::Fill::new(&lifetime, &only, false);
            for arg in &mut sig.inputs {
                fill.visit_fn_arg_mut(arg);
            }
            if fill.count == 0 {
                let msg = format!(
                    "`from({name})` borrows from a `{name}` parameter, \
                     but no parameter here has type `{name}` without a lifetime"
                );
                combine(&mut errors, syn::Error::new_spanned(&marker.written, msg));
            }
        }
    }
    for arg in &mut sig.inputs {
        resolve.visit_fn_arg_mut(arg);
    }
    resolve.visit_return_type_mut(&mut sig.output);
    if let Some(err) = resolve.errors {
        combine(&mut errors, err);
    }
    errors.map_or(Ok(()), Err)
}

/// Where a new lifetime parameter goes: after the lifetimes the signature already declares.
fn lifetimes_end(generics: &Generics) -> usize {
    generics
        .params
        .iter()
        .take_while(|param| matches!(param, GenericParam::Lifetime(_)))
        .count()
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
    if let Some(ident) = bare_name(ty)
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
                && bare_name(&predicate.bounded_ty) == Some(ident)
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
                    combine(&mut self.errors, syn::Error::new(lifetime.apostrophe, msg));
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

    /// Runs `#[borrows]` first, as `passes/mod.rs` orders them.
    fn borrowed(sugar: proc_macro2::TokenStream) -> syn::Result<String> {
        let tokens = super::super::rewrite(sugar)?;
        let mut file: syn::File = syn::parse2(tokens)?;
        borrows::lower(&mut file)?;
        lower(&mut file)?;
        Ok(file.into_token_stream().to_string())
    }

    fn borrowed_errors(sugar: proc_macro2::TokenStream) -> Vec<String> {
        borrowed(sugar)
            .unwrap_err()
            .into_iter()
            .map(|err| err.to_string())
            .collect()
    }

    #[test]
    fn from_self_in_a_borrowing_impl_is_the_type_lifetime_in_parameters_and_return() {
        assert_eq!(
            borrowed(quote! {
                #[borrows] struct Lexer { src: &str }
                impl Lexer {
                    fn new(src: &from(Self) str) -> Lexer { Lexer { src } }
                    // What `prefix_types` makes of `iter &from(Self) str`.
                    fn words(&self) -> impl Iterator<Item = &from(Self) str> + '_ { self.src.split(' ') }
                    fn get(&self, k: &str) -> Option<&from(self) str> { None }
                }
            })
            .unwrap(),
            printed(quote! {
                struct Lexer<'__rustx_borrows> { src: &'__rustx_borrows str }
                impl<'__rustx_borrows> Lexer<'__rustx_borrows> {
                    fn new(src: &'__rustx_borrows str) -> Lexer<'__rustx_borrows> { Lexer { src } }
                    fn words(&self) -> impl Iterator<Item = &'__rustx_borrows str> + '__rustx_borrows {
                        self.src.split(' ')
                    }
                    fn get<'__rustx_from_self>(&'__rustx_from_self self, k: &str)
                        -> Option<&'__rustx_from_self str> { None }
                }
            })
        );
    }

    #[test]
    fn from_type_outside_its_impl_adds_the_lifetime_and_ties_that_parameter() {
        let expanded = borrowed(quote! {
            #[borrows] struct Parser { src: &str }
            fn first<'x>(p: &mut Parser, q: &Parser, s: &'x str) -> Option<&from(Parser) str> { None }
        })
        .unwrap();
        assert!(
            expanded.ends_with(&printed(quote! {
                fn first<'x, '__rustx_borrows>(
                    p: &mut Parser<'__rustx_borrows>,
                    q: &Parser<'__rustx_borrows>,
                    s: &'x str
                ) -> Option<&'__rustx_borrows str> { None }
            })),
            "{expanded}"
        );
    }

    #[test]
    fn from_self_outside_a_borrowing_impl_is_an_error() {
        assert_eq!(
            borrowed_errors(quote! {
                #[borrows] struct Lexer { src: &str }
                impl Lexer { fn f() { fn inner(s: &str) -> &from(Self) str { s } } }
                struct Plain;
                impl Plain { fn f(s: &str) -> &from(Self) str { s } }
                fn g(s: &str) -> &from(Self) str { s }
            }),
            ["`from(Self)` only works in an `impl` of a `#[borrows]` type"; 3]
        );
    }

    #[test]
    fn from_type_without_such_a_parameter_or_with_other_names_is_an_error() {
        assert_eq!(
            borrowed_errors(quote! {
                #[borrows] struct Parser { src: &str }
                fn f(p: &Parser<'static>) -> &from(Parser) str { "" }
                impl Parser { fn g(&self) -> &from(Self, self) str { "" } }
            }),
            [
                "`from(Parser)` borrows from a `Parser` parameter, but no parameter here has type \
                 `Parser` without a lifetime",
                "`from(Self)` names a type's lifetime, so it takes no other names",
            ]
        );
    }

    #[test]
    fn from_a_type_without_borrows_still_names_no_parameter() {
        assert_eq!(
            borrowed_errors(quote! { fn f(p: &Parser) -> &from(Parser) str { "" } }),
            ["`from(..)` names `Parser`, which is not a parameter here"]
        );
    }
}
