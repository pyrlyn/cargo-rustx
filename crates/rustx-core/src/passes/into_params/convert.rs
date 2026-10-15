//! AST half of `into T` parameters: every marked parameter gets the type
//! `impl ::core::convert::Into<T>`, and a body, when there is one, starts with the conversion.
//!
//! The parameter's binding mode (`mut`, `ref`) moves to the `let`, where the converted value
//! lives; on the parameter it would only trigger `unused_mut`. Its other attributes (`cfg`
//! and lint levels, the only ones Rust allows on a parameter) are copied to the `let`, so a
//! parameter that is configured out takes its conversion with it.

use proc_macro2::Span;
use quote::quote_spanned;
use syn::spanned::Spanned;
use syn::visit_mut::{self, VisitMut};
use syn::{
    Attribute, Block, FnArg, ForeignItemFn, ImplItemFn, ItemFn, Pat, PatIdent, PatType, Signature,
    Stmt, TraitItemFn,
};

use super::marker::MARKER;
use crate::tokens::syntax::combine;

pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    let mut lower = Lower { errors: None };
    lower.visit_file_mut(file);
    lower.errors.map_or(Ok(()), Err)
}

struct Lower {
    errors: Option<syn::Error>,
}

impl Lower {
    /// Rewrites the marked parameters of `sig`; `body` is `None` for a declaration, which only
    /// states the type: whoever implements it converts in their own body.
    fn convert(&mut self, sig: &mut Signature, body: Option<&mut Block>) {
        let mut conversions = Vec::new();
        for arg in &mut sig.inputs {
            match arg {
                FnArg::Typed(param) => match convert_parameter(param) {
                    Ok(Some(stmt)) => conversions.push(stmt),
                    Ok(None) => {}
                    Err(err) => combine(&mut self.errors, err),
                },
                FnArg::Receiver(receiver) => {
                    if let Some(into) = take_marker(&mut receiver.attrs) {
                        let message = "`self` cannot take `into`; convert it in the body";
                        combine(&mut self.errors, syn::Error::new(into, message));
                    }
                }
            }
        }
        if let Some(body) = body {
            body.stmts.splice(0..0, conversions);
        }
    }
}

impl VisitMut for Lower {
    fn visit_item_fn_mut(&mut self, item: &mut ItemFn) {
        self.convert(&mut item.sig, Some(&mut item.block));
        visit_mut::visit_item_fn_mut(self, item);
    }

    fn visit_impl_item_fn_mut(&mut self, item: &mut ImplItemFn) {
        self.convert(&mut item.sig, Some(&mut item.block));
        visit_mut::visit_impl_item_fn_mut(self, item);
    }

    fn visit_trait_item_fn_mut(&mut self, item: &mut TraitItemFn) {
        self.convert(&mut item.sig, item.default.as_mut());
        visit_mut::visit_trait_item_fn_mut(self, item);
    }

    // Rewritten like a trait declaration so rustc, not an unknown marker, reports that an
    // `extern` fn cannot take `impl Trait`; the error then points at `into`.
    fn visit_foreign_item_fn_mut(&mut self, item: &mut ForeignItemFn) {
        self.convert(&mut item.sig, None);
        visit_mut::visit_foreign_item_fn_mut(self, item);
    }
}

/// Removes the marker of a parameter and returns the span of its `into`.
fn take_marker(attrs: &mut Vec<Attribute>) -> Option<Span> {
    let at = attrs.iter().position(|attr| attr.path().is_ident(MARKER))?;
    Some(attrs.remove(at).pound_token.spans[0])
}

/// `#[m] x: T` → `x: impl Into<T>`, returning `let x: T = Into::into(x);` for the body.
fn convert_parameter(param: &mut PatType) -> syn::Result<Option<Stmt>> {
    let Some(into) = take_marker(&mut param.attrs) else {
        return Ok(None);
    };
    let Pat::Ident(PatIdent {
        by_ref,
        mutability,
        ident,
        subpat: None,
        ..
    }) = &mut *param.pat
    else {
        return Err(not_a_name(&param.pat));
    };
    // Other passes' markers stay on the parameter only; they are not Rust attributes.
    let attrs = param.attrs.iter().filter(|attr| !is_rustx_marker(attr));
    let target = &param.ty;
    // Fully qualified, so an inherent or other trait method named `into` cannot be picked.
    let stmt = syn::parse2(quote_spanned! {into=>
        #(#attrs)* let #by_ref #mutability #ident: #target = ::core::convert::Into::into(#ident);
    })?;
    param.ty = syn::parse2(quote_spanned!(into=> impl ::core::convert::Into<#target>))?;
    *by_ref = None;
    *mutability = None;
    Ok(Some(stmt))
}

fn is_rustx_marker(attr: &Attribute) -> bool {
    attr.path()
        .get_ident()
        .is_some_and(|name| name.to_string().starts_with("__rustx_"))
}

fn not_a_name(pat: &Pat) -> syn::Error {
    syn::Error::new(
        pat.span(),
        "`into` needs a plain parameter name such as `name`, not a pattern",
    )
}
