//! `except(..)`: forwarding into a hand-written impl of the trait.
//!
//! `#[delegate(Write, except(write))]` forwards every method but `write`, which the user writes
//! in an `impl Write for Struct` next to the struct (problem 07's `CountingWriter`). The forwarded
//! items are added to that impl, matched by the trait's and the type's last path segment, in
//! the same item list (file, module or block) as the struct.
//!
//! Without `except`, a hand-written impl of the trait next to the struct receives the items too,
//! but a method written there by hand must be listed in `except`, so the attribute says which
//! methods are not forwarded.

use proc_macro2::Ident;
use syn::{ImplItem, Item, ItemImpl, Path, TraitItem, Type};

use super::last_segment;
use super::spec::Delegation;
use crate::tokens::syntax::combine;

/// Every name in `except(..)` must be a method the trait forwards.
pub(super) fn check(delegation: &Delegation, trait_items: &[TraitItem]) -> syn::Result<()> {
    let mut errors = None;
    for name in &delegation.except {
        let known = trait_items
            .iter()
            .any(|item| matches!(item, TraitItem::Fn(method) if method.sig.ident == *name));
        if !known {
            let message = format!(
                "`{name}` is not a forwarded method of `{}`",
                last_segment(&delegation.path)
            );
            combine(&mut errors, syn::Error::new(name.span(), message));
        }
    }
    errors.map_or(Ok(()), Err)
}

/// Starts the error for `except(..)` without an impl; the standalone attribute rewords it.
pub(crate) const EXCEPT_WITHOUT_IMPL: &str = "`except(..)` needs a hand-written";

/// An error when `except(..)` has no hand-written impl to leave its methods to.
pub(super) fn without_impl(delegation: &Delegation, owner: &Ident) -> syn::Result<()> {
    let Some(keyword) = delegation.except_span else {
        return Ok(());
    };
    let message = format!(
        "{EXCEPT_WITHOUT_IMPL} `impl {} for {owner}` next to the struct",
        last_segment(&delegation.path),
    );
    Err(syn::Error::new(keyword, message))
}

/// `item`, if it is an impl of the trait `path` for the struct `owner`.
pub(super) fn hand_written<'a>(
    item: &'a mut Item,
    owner: &Ident,
    path: &Path,
) -> Option<&'a mut ItemImpl> {
    let Item::Impl(existing) = item else {
        return None;
    };
    let (trait_path, _) = existing.trait_.as_ref()?;
    let Type::Path(self_ty) = &*existing.self_ty else {
        return None;
    };
    let same_type = self_ty
        .path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == *owner);
    (same_type
        && existing.modifiers.polarity.is_none()
        && last_segment(trait_path) == last_segment(path))
    .then_some(existing)
}

/// Adds forwarded items to a hand-written impl. An associated type or const written there wins;
/// a method written there and not listed in `except` is an error.
pub(super) fn merge(existing: &mut ItemImpl, items: Vec<ImplItem>) -> syn::Result<()> {
    let mut errors = None;
    for item in items {
        match existing.items.iter().find(|have| name(have) == name(&item)) {
            Some(ImplItem::Fn(method)) => {
                let ident = &method.sig.ident;
                let message = format!(
                    "`{ident}` is written by hand and also delegated; add it to `except(..)`"
                );
                combine(&mut errors, syn::Error::new(ident.span(), message));
            }
            Some(_) => {}
            None => existing.items.push(item),
        }
    }
    errors.map_or(Ok(()), Err)
}

fn name(item: &ImplItem) -> Option<String> {
    match item {
        ImplItem::Fn(method) => Some(method.sig.ident.to_string()),
        ImplItem::Type(assoc) => Some(assoc.ident.to_string()),
        ImplItem::Const(constant) => Some(constant.ident.to_string()),
        _ => None,
    }
}
