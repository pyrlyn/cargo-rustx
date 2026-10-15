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
use syn::{Attribute, Block, FnArg, Pat, PatIdent, PatType, Signature, Stmt};

use super::marker::MARKER;
use crate::passes::signatures::for_each_signature;
use crate::tokens::syntax::{combine, is_marker, take_attr};

// `extern` fns are rewritten like trait declarations, so rustc, not an unknown marker, reports
// that they cannot take `impl Trait`; the error then points at `into`.
pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    for_each_signature(file, convert)
}

/// Rewrites the marked parameters of `sig`; `body` is `None` for a declaration, which only
/// states the type: whoever implements it converts in their own body.
fn convert(sig: &mut Signature, body: Option<&mut Block>) -> syn::Result<()> {
    let mut errors = None;
    let mut conversions = Vec::new();
    for arg in &mut sig.inputs {
        match arg {
            FnArg::Typed(param) => match convert_parameter(param) {
                Ok(Some(stmt)) => conversions.push(stmt),
                Ok(None) => {}
                Err(err) => combine(&mut errors, err),
            },
            FnArg::Receiver(receiver) => {
                if let Some(into) = take_marker(&mut receiver.attrs) {
                    let message = "`self` cannot take `into`; convert it in the body";
                    combine(&mut errors, syn::Error::new(into, message));
                }
            }
        }
    }
    if let Some(body) = body {
        body.stmts.splice(0..0, conversions);
    }
    errors.map_or(Ok(()), Err)
}

/// Removes the marker of a parameter and returns the span of its `into`.
fn take_marker(attrs: &mut Vec<Attribute>) -> Option<Span> {
    take_attr(attrs, MARKER).map(|marker| marker.pound_token.spans[0])
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
    let attrs = param.attrs.iter().filter(|attr| !is_marker(attr));
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

fn not_a_name(pat: &Pat) -> syn::Error {
    syn::Error::new(
        pat.span(),
        "`into` needs a plain parameter name such as `name`, not a pattern",
    )
}
