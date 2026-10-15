//! AST half of parameter defaults: every marked parameter of a fn with a body becomes
//! `Option<T>`, and the body starts by unwrapping it.
//!
//! Separate from the token half because it needs the parsed signature and body: the pattern
//! decides the binding name, the expression decides between `unwrap_or` and `unwrap_or_else`.

use proc_macro2::{Span, TokenStream};
use quote::{format_ident, quote_spanned};
use syn::spanned::Spanned;
use syn::visit_mut::{self, VisitMut};
use syn::{
    Attribute, Block, Expr, FnArg, ForeignItemFn, ImplItemFn, ItemFn, Pat, PatIdent, PatType,
    Signature, Stmt, TraitItemFn, Type, TypeImplTrait, UnOp,
};

use super::marker::MARKER;
use super::{ParamDefault, default_of};
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
    fn error(&mut self, span: Span, message: &str) {
        combine(&mut self.errors, syn::Error::new(span, message));
    }

    /// Lowers the defaults of one signature; `body` is `None` for a declaration without one.
    fn signature(&mut self, sig: &mut Signature, body: Option<&mut Block>) {
        let mut unwraps = Vec::new();
        let mut first_default: Option<Span> = None;
        for (index, arg) in sig.inputs.iter_mut().enumerate() {
            match arg {
                FnArg::Receiver(receiver) => {
                    if let Some(default) = take(&mut receiver.attrs) {
                        self.error(default.eq, "`self` takes no default value");
                    }
                }
                FnArg::Typed(param) => match take(&mut param.attrs) {
                    Some(ParamDefault { eq, expr }) => {
                        first_default.get_or_insert(eq);
                        match expr.and_then(|expr| unwrap(param, index, eq, &expr)) {
                            Ok(stmt) => unwraps.push(stmt),
                            Err(err) => combine(&mut self.errors, err),
                        }
                    }
                    None if first_default.is_some() => self.error(
                        param.pat.span(),
                        "a parameter without a default cannot follow one with a default; \
                         move it before the defaulted parameters",
                    ),
                    None => {}
                },
            }
        }
        match (body, first_default) {
            (Some(body), _) => {
                body.stmts.splice(0..0, unwraps);
            }
            (None, Some(eq)) => self.error(
                eq,
                "a parameter default needs a fn body to apply it; declarations without a body \
                 take `Option<T>` instead",
            ),
            (None, None) => {}
        }
    }
}

impl VisitMut for Lower {
    fn visit_item_fn_mut(&mut self, item: &mut ItemFn) {
        self.signature(&mut item.sig, Some(&mut item.block));
        visit_mut::visit_item_fn_mut(self, item);
    }

    fn visit_impl_item_fn_mut(&mut self, item: &mut ImplItemFn) {
        self.signature(&mut item.sig, Some(&mut item.block));
        visit_mut::visit_impl_item_fn_mut(self, item);
    }

    fn visit_trait_item_fn_mut(&mut self, item: &mut TraitItemFn) {
        self.signature(&mut item.sig, item.default.as_mut());
        visit_mut::visit_trait_item_fn_mut(self, item);
    }

    fn visit_foreign_item_fn_mut(&mut self, item: &mut ForeignItemFn) {
        self.signature(&mut item.sig, None);
        visit_mut::visit_foreign_item_fn_mut(self, item);
    }
}

/// Removes the marker from a parameter and returns the default it carried.
fn take(attrs: &mut Vec<Attribute>) -> Option<ParamDefault> {
    let default = default_of(attrs);
    attrs.retain(|attr| !attr.path().is_ident(MARKER));
    default
}

/// Turns `pat: T` into `name: Option<T>` and returns `let pat: T = name.unwrap_or(expr);`.
///
/// A plain binding keeps its name, so the public signature stays readable; any other pattern
/// (`(a, b)`, `ref x`, `_`) moves into the `let` and the parameter gets a generated name.
fn unwrap(param: &mut PatType, index: usize, eq: Span, expr: &Expr) -> syn::Result<Stmt> {
    let pat = (*param.pat).clone();
    let binding = match &mut *param.pat {
        Pat::Ident(PatIdent {
            by_ref: None,
            subpat: None,
            mutability,
            ident,
            ..
        }) => {
            // `mut` belongs to the unwrapped value in the body, not to the `Option`.
            *mutability = None;
            ident.clone()
        }
        other => {
            let binding = format_ident!("__rustx_default_{}", index, span = eq);
            *other = Pat::Ident(PatIdent {
                attrs: Vec::new(),
                by_ref: None,
                mutability: None,
                ident: binding.clone(),
                subpat: None,
            });
            binding
        }
    };
    // Cheap, side-effect-free defaults are passed by value; anything else runs only when the
    // argument is `None`.
    let value = if is_constant(expr) {
        quote_spanned!(eq=> #binding.unwrap_or(#expr))
    } else {
        quote_spanned!(eq=> #binding.unwrap_or_else(|| #expr))
    };
    let ty = &*param.ty;
    // `impl Trait` is allowed inside `Option<..>` in argument position, but not in a `let`.
    let annotation = if has_impl_trait(ty) {
        TokenStream::new()
    } else {
        quote_spanned!(eq=> : #ty)
    };
    let stmt = syn::parse2(quote_spanned!(eq=> let #pat #annotation = #value;))?;
    *param.ty = syn::parse2(quote_spanned!(eq=> ::core::option::Option<#ty>))?;
    Ok(stmt)
}

/// Whether `expr` is a literal, a path, or arithmetic, casts, tuples and arrays of those.
fn is_constant(expr: &Expr) -> bool {
    match expr {
        Expr::Lit(_) | Expr::Path(_) => true,
        Expr::Unary(unary) => {
            matches!(unary.op, UnOp::Neg(_) | UnOp::Not(_)) && is_constant(&unary.expr)
        }
        Expr::Binary(binary) => is_constant(&binary.left) && is_constant(&binary.right),
        Expr::Cast(cast) => is_constant(&cast.expr),
        Expr::Paren(paren) => is_constant(&paren.expr),
        Expr::Group(group) => is_constant(&group.expr),
        Expr::Tuple(tuple) => tuple.elems.iter().all(is_constant),
        Expr::Array(array) => array.elems.iter().all(is_constant),
        _ => false,
    }
}

fn has_impl_trait(ty: &Type) -> bool {
    // Searching a copy through `VisitMut` because the crate enables only that visitor.
    struct Finds(bool);
    impl VisitMut for Finds {
        fn visit_type_impl_trait_mut(&mut self, _: &mut TypeImplTrait) {
            self.0 = true;
        }
    }
    let mut finds = Finds(false);
    finds.visit_type_mut(&mut ty.clone());
    finds.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;

    fn constant(source: &str) -> bool {
        syn::parse_str::<Expr>(source).is_ok_and(|expr| is_constant(&expr))
    }

    #[test]
    fn literals_paths_and_their_arithmetic_are_constant() {
        for source in [
            "8080",
            "-1",
            "u16::MAX",
            "None",
            "60 * 1000",
            "(1, [2, 3])",
            "1 as u8",
        ] {
            assert!(constant(source), "{source}");
        }
    }

    #[test]
    fn calls_macros_and_blocks_are_not_constant() {
        for source in [
            "String::new()",
            "vec![1]",
            "x.len()",
            "{ 1 }",
            "*r",
            "&1",
            "|| 1",
        ] {
            assert!(!constant(source), "{source}");
        }
    }

    #[test]
    fn impl_trait_is_found_anywhere_in_a_type() {
        let has = |source: &str| syn::parse_str::<Type>(source).is_ok_and(|ty| has_impl_trait(&ty));
        assert!(has("impl Display") && has("Vec<impl Display>") && has("(u8, impl Tr)"));
        assert!(!has("Box<dyn Display>") && !has("u8"));
    }

    #[test]
    fn a_marker_on_a_parameter_is_read_without_being_removed() {
        let item: ItemFn = syn::parse_quote! { fn f(#[__rustx_default(1 + 2)] x: u8) {} };
        let Some(FnArg::Typed(param)) = item.sig.inputs.first() else {
            unreachable!()
        };
        let default = default_of(&param.attrs).map(|default| default.expr.ok());
        assert_eq!(
            default
                .flatten()
                .map(|expr| expr.to_token_stream().to_string()),
            Some("1 + 2".to_owned())
        );
        assert_eq!(param.attrs.len(), 1);
    }
}
