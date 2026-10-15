//! Impl items that forward a trait's items to one field.

use proc_macro2::{Ident, Span, TokenStream};
use quote::{format_ident, quote_spanned};
use syn::{
    FnArg, ImplItem, Member, Path, PathArguments, ReceiverKind, Token, TraitItem, TraitItemFn, Type,
};

use crate::tokens::syntax::{RESERVED_PREFIX, introduced_span};

/// The field the items forward to, and the trait they implement.
pub(super) struct Target<'a> {
    pub(super) field: &'a Member,
    pub(super) ty: &'a Type,
    pub(super) path: &'a Path,
    pub(super) span: Span,
}

/// One forwarding item per trait item, skipping the methods named in `except`.
///
/// Calls go through the trait path (`Trait::method(&mut self.field, ..)`), so an inherent
/// method of the field type with the same name cannot capture them. Only associated types and
/// consts name the field type, which keeps its elided lifetimes out of method bodies.
pub(super) fn items(
    target: &Target,
    trait_items: &[TraitItem],
    except: &[Ident],
) -> syn::Result<Vec<ImplItem>> {
    let Target { ty, path, span, .. } = target;
    let mut out = Vec::new();
    for item in trait_items {
        let tokens = match item {
            TraitItem::Fn(method) if except.contains(&method.sig.ident) => continue,
            TraitItem::Fn(method) => method_tokens(target, method)?,
            TraitItem::Type(assoc) => {
                if !assoc.generics.params.is_empty() {
                    return Err(syn::Error::new(
                        assoc.ident.span(),
                        "generic associated types are not delegated; implement the trait by hand",
                    ));
                }
                let ident = &assoc.ident;
                quote_spanned!(*span=> type #ident = <#ty as #path>::#ident;)
            }
            TraitItem::Const(constant) => {
                let (ident, const_ty) = (&constant.ident, &constant.ty);
                quote_spanned!(*span=> const #ident: #const_ty = <#ty as #path>::#ident;)
            }
            _ => continue,
        };
        out.push(syn::parse2(tokens)?);
    }
    Ok(out)
}

fn method_tokens(target: &Target, method: &TraitItemFn) -> syn::Result<TokenStream> {
    let Target {
        field, path, span, ..
    } = target;
    let mut sig = method.sig.clone();
    let mut receiver = None;
    let mut args = Vec::new();
    for input in &mut sig.inputs {
        match input {
            FnArg::Receiver(this) => {
                receiver = Some(match &this.kind {
                    ReceiverKind::Value => quote_spanned!(*span=> self.#field),
                    ReceiverKind::Reference(_, _, None) => quote_spanned!(*span=> &self.#field),
                    ReceiverKind::Reference(_, _, Some(_)) => {
                        quote_spanned!(*span=> &mut self.#field)
                    }
                    _ => {
                        return Err(syn::Error::new(
                            this.self_token.span,
                            "only `self`, `&self` and `&mut self` methods are delegated",
                        ));
                    }
                });
                // The body's `self` is generated here, so the receiver must resolve alike.
                this.self_token.span = *span;
                this.mutability = None;
                this.attrs.clear();
            }
            FnArg::Typed(typed) => {
                // Trait declarations may bind patterns or `_`; the call needs one name each.
                let name = format_ident!(
                    "{RESERVED_PREFIX}arg{}",
                    args.len(),
                    span = introduced_span(*span)
                );
                *typed.pat = syn::parse_quote!(#name);
                typed.attrs.clear();
                args.push(name);
            }
        }
    }
    let Some(receiver) = receiver else {
        return Err(syn::Error::new(
            sig.ident.span(),
            format!(
                "`{}` takes no `self`, so it cannot be forwarded to a field",
                sig.ident
            ),
        ));
    };
    // In expression position the trait's own arguments need a turbofish: `AsRef::<str>::as_ref`.
    let mut callee = (*path).clone();
    if let Some(last) = callee.segments.last_mut()
        && let PathArguments::AngleBracketed(arguments) = &mut last.arguments
    {
        arguments.colon2_token = Some(Token![::](*span));
    }
    let ident = &sig.ident;
    let call = quote_spanned!(*span=> #callee::#ident(#receiver #(, #args)*));
    let call = if sig.asyncness.is_some() {
        quote_spanned!(*span=> #call.await)
    } else {
        call
    };
    Ok(quote_spanned!(*span=> #[inline] #sig { #call }))
}
