//! `fn f<T>(x: T) where T: Bound` back to `fn f(x: T: Bound)`: the reverse of
//! `passes/inline_bounds.rs` (problem 3, T19.9).
//!
//! For `x: T: B` with a name not in scope, the pass appends `T` to the fn's generics and `T: B`
//! to its `where` clause, both after whatever is already there and in parameter order. A generic
//! converts only when the signature has exactly that shape: a plain `T` (no bounds, default or
//! attribute) among the trailing generics, a plain `T: B` (no `for<..>`) among the trailing
//! predicates, the two in the order of their parameters, and neither list ending in a comma,
//! which the pass never writes. rustfmt ends a multi-line `where` with one, so formatted code
//! keeps its generics.
//!
//! The generic must be the whole type of exactly one parameter and be mentioned nowhere else in
//! the fn: not in the return type, another parameter, another bound or the body. A name the pass
//! reads as a type (declared or imported by the file, a primitive or a prelude type) would be an
//! error as sugar, so it stays a generic; so does one whose removed text holds a comment.
//!
//! The generic, the parameter and the predicate are three places in the text but one edit,
//! spanning from the first removed generic to the last removed predicate, so they are kept or
//! dropped together: the driver's one-at-a-time fallback could otherwise keep a part that does
//! not expand back on its own, or drop parts that only expand back together. Another rule's edit
//! inside that span (a parameter or return type of the same fn) is dropped by the driver. A
//! generic that also names one of the enclosing `impl` (which rustc rejects) is not checked
//! here: the pass would only add the bound, and the driver refuses the edit.

use std::ops::Range;

use proc_macro2::{Ident, TokenStream};
use quote::ToTokens;
use syn::visit::{self, Visit};
use syn::{
    FnArg, GenericParam, ImplItemFn, ItemFn, PredicateType, ReturnType, Signature, TraitItemFn,
    TypeParam, WherePredicate,
};

use super::{Edit, ReverseRule, apply, bounds, leaves};
use crate::passes::KnownTypes;
use crate::tokens::syntax::bare_name;

pub(crate) struct InlineBounds;

impl ReverseRule for InlineBounds {
    fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit> {
        let mut finder = Finder {
            source,
            known: KnownTypes::of(file),
            edits: Vec::new(),
        };
        finder.visit_file(file);
        finder.edits
    }
}

struct Finder<'a> {
    source: &'a str,
    known: KnownTypes,
    edits: Vec<Edit>,
}

/// A generic the pass would have written for one parameter.
struct Inline<'s> {
    /// Index of the parameter in the fn's inputs.
    param: usize,
    /// Byte offset just after the parameter's type, where the bounds go.
    after_type: usize,
    predicate: &'s PredicateType,
}

impl Finder<'_> {
    fn convert(&mut self, sig: &Signature, item: &TokenStream) {
        if let Some(edit) = self.edit(sig, item) {
            self.edits.push(edit);
        }
    }

    /// One edit for every generic of `sig` that converts, if any does.
    fn edit(&self, sig: &Signature, item: &TokenStream) -> Option<Edit> {
        let generics = &sig.generics;
        let clause = generics.where_clause.as_ref()?;
        if generics.params.trailing_punct() || clause.predicates.trailing_punct() {
            return None;
        }
        let params: Vec<&GenericParam> = generics.params.iter().collect();
        let predicates: Vec<&WherePredicate> = clause.predicates.iter().collect();

        // From the end of both lists, since the pass appends; each one taken must belong to a
        // parameter before the one taken after it.
        let mut taken: Vec<Inline> = Vec::new();
        for (param, predicate) in params.iter().rev().zip(predicates.iter().rev()) {
            let Some(inline) = self.inline(sig, param, predicate, item) else {
                break;
            };
            if taken
                .last()
                .is_some_and(|later| inline.param >= later.param)
            {
                break;
            }
            taken.push(inline);
        }
        if taken.is_empty() {
            return None;
        }
        taken.reverse();
        let count = taken.len();
        let first_taken = params.len().checked_sub(count)?;
        let first_predicate = predicates.len().checked_sub(count)?;
        let end = |tokens: TokenStream| bounds(tokens).map(|(_, last)| last.byte_range().end);

        let generics_range = if first_taken == 0 {
            let (lt, gt) = (generics.lt_token?, generics.gt_token?);
            lt.span.byte_range().start..gt.span.byte_range().end
        } else {
            end(params[first_taken - 1].to_token_stream())?..end(params.last()?.to_token_stream())?
        };
        let where_start = if first_predicate == 0 {
            match &sig.output {
                ReturnType::Type(_, ty) => end(ty.to_token_stream())?,
                ReturnType::Default => sig.paren_token.span.close().byte_range().end,
            }
        } else {
            end(predicates[first_predicate - 1].to_token_stream())?
        };
        let where_range = where_start..end(predicates.last()?.to_token_stream())?;

        let mut moved = Vec::with_capacity(count);
        for inline in &taken {
            let (first, last) = bounds(inline.predicate.bounds.to_token_stream())?;
            moved.push(first.byte_range().start..last.byte_range().end);
        }
        if self.loses_a_comment(&generics_range, &where_range, &moved) {
            return None;
        }

        let range = generics_range.start..where_range.end;
        let relative = |at: &Range<usize>, text: String| Edit {
            range: at.start - range.start..at.end - range.start,
            text,
        };
        let mut parts = vec![relative(&generics_range, String::new())];
        for (inline, bounds) in taken.iter().zip(&moved) {
            let text = format!(": {}", self.source.get(bounds.clone())?);
            parts.push(relative(&(inline.after_type..inline.after_type), text));
        }
        parts.push(relative(&where_range, String::new()));
        let text = apply(self.source.get(range.clone())?, &parts);
        Some(Edit { range, text })
    }

    /// `param` with `predicate` when the pass would have written both for a parameter of `sig`.
    fn inline<'s>(
        &self,
        sig: &Signature,
        param: &GenericParam,
        predicate: &'s WherePredicate,
        item: &TokenStream,
    ) -> Option<Inline<'s>> {
        let GenericParam::Type(TypeParam {
            attrs,
            ident,
            colon_token: None,
            default: None,
            ..
        }) = param
        else {
            return None;
        };
        let WherePredicate::Type(predicate) = predicate else {
            return None;
        };
        let plain = attrs.is_empty()
            && predicate.attrs.is_empty()
            && predicate.lifetimes.is_none()
            && !predicate.bounds.trailing_punct()
            && bare_name(&predicate.bounded_ty) == Some(ident);
        // The declaration, the predicate and the one parameter: any other mention stays generic.
        if !plain || self.known.is_type(ident) || mentions(item, ident) != 3 {
            return None;
        }
        sig.inputs
            .iter()
            .enumerate()
            .find_map(|(at, arg)| match arg {
                FnArg::Typed(typed) => bare_name(&typed.ty)
                    .filter(|name| *name == ident)
                    .map(|name| (at, name.span().byte_range().end)),
                FnArg::Receiver(_) => None,
            })
            .map(|(param, after_type)| Inline {
                param,
                after_type,
                predicate,
            })
    }

    /// Whether the removed text, apart from the bounds that move, holds anything but the
    /// tokens the pass writes, which contain no `/`: a comment would be lost.
    fn loses_a_comment(
        &self,
        generics: &Range<usize>,
        clause: &Range<usize>,
        moved: &[Range<usize>],
    ) -> bool {
        let mut removed = vec![generics.clone()];
        let mut at = clause.start;
        for bounds in moved {
            removed.push(at..bounds.start);
            at = bounds.end;
        }
        removed.push(at..clause.end);
        removed
            .into_iter()
            .any(|range| self.source.get(range).is_none_or(|text| text.contains('/')))
    }
}

/// How many tokens of `item` are the identifier `name`.
fn mentions(item: &TokenStream, name: &Ident) -> usize {
    let mut all = Vec::new();
    leaves(item.clone(), &mut all);
    let name = name.to_string();
    all.iter().filter(|(token, _)| *token == name).count()
}

impl<'ast> Visit<'ast> for Finder<'_> {
    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        self.convert(&item.sig, &item.to_token_stream());
        visit::visit_item_fn(self, item);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        self.convert(&item.sig, &item.to_token_stream());
        visit::visit_impl_item_fn(self, item);
    }

    fn visit_trait_item_fn(&mut self, item: &'ast TraitItemFn) {
        self.convert(&item.sig, &item.to_token_stream());
        visit::visit_trait_item_fn(self, item);
    }
}

#[cfg(test)]
mod tests {
    use super::InlineBounds;
    use crate::sugar::tests::assert_sugar;
    use crate::sugar::{ReverseRule, sugar};

    fn edits(source: &str) -> usize {
        let file = syn::parse_file(source).unwrap();
        InlineBounds.edits(&file, source).len()
    }

    #[test]
    fn a_generic_bounded_in_where_becomes_an_inline_bound() {
        assert_sugar(
            "fn run<T>(x: T) where T: Clone + Debug + Send { drop(x); }\n",
            "fn run(x: T: Clone + Debug + Send) { drop(x); }\n",
        );
    }

    #[test]
    fn existing_generics_and_predicates_stay_in_place() {
        assert_sugar(
            "fn f<'a, U: Copy, T>(u: &'a U, mut x: T) where U: Debug, T: Clone {}\n",
            "fn f<'a, U: Copy>(u: &'a U, mut x: T: Clone) where U: Debug {}\n",
        );
    }

    #[test]
    fn several_generics_convert_together_in_parameter_order() {
        assert_sugar(
            "fn g<A, B>(a: A, n: u8, b: B) -> u8 where A: Clone, B: Iterator<Item = u8> { n }\n",
            "fn g(a: A: Clone, n: u8, b: B: Iterator<Item = u8>) -> u8 { n }\n",
        );
    }

    #[test]
    fn methods_trait_fns_and_nested_fns_convert() {
        assert_sugar(
            "impl S {\n    fn m<T>(&self, x: T) where T: Copy {\n        \
             fn n<U>(u: U) where U: Send {}\n    }\n}\n\
             trait Tr {\n    fn t<T>(x: T) where T: Copy;\n}\n",
            "impl S {\n    fn m(&self, x: T: Copy) {\n        \
             fn n(u: U: Send) {}\n    }\n}\n\
             trait Tr {\n    fn t(x: T: Copy);\n}\n",
        );
    }

    #[test]
    fn only_the_trailing_generics_in_parameter_order_convert() {
        // `A` is also the return type, so it stays and `B` is the only one the pass appended.
        assert_sugar(
            "fn f<A, B>(a: A, b: B) -> A where A: Clone, B: Send { a }\n",
            "fn f<A>(a: A, b: B: Send) -> A where A: Clone { a }\n",
        );
        // Declared in the other order than their parameters: the pass would append `B` after
        // `A`, so only the last one converts.
        assert_sugar(
            "fn f<B, A>(a: A, b: B) where B: Send, A: Clone {}\n",
            "fn f<B>(a: A: Clone, b: B) where B: Send {}\n",
        );
        // The last generic and the last predicate are about different names.
        let mismatched = "fn f<A, B>(a: A, b: B) where B: Send, A: Clone {}\n";
        assert_eq!(edits(mismatched), 0);
    }

    #[test]
    fn a_generic_used_anywhere_else_stays() {
        let source = "fn ret<T>(x: T) -> T where T: Clone { x }\n\
                      fn two<T>(x: T, y: T) where T: Clone {}\n\
                      fn nested<T>(x: Vec<T>) where T: Clone {}\n\
                      fn body<T>(x: T) where T: Clone { let _ = T::default(); }\n\
                      fn turbofish<T>(x: T) where T: Into<u8> { f::<T>(); }\n\
                      fn other<T, U>(x: T, u: U) where U: From<T>, T: Clone {}\n\
                      fn split<T>(x: T) where T: Clone, T: Send {}\n\
                      fn recursive<T>(x: T) where T: PartialEq<T> {}\n\
                      fn receiver<T>(self: T) where T: Clone {}\n";
        assert_eq!(edits(source), 0);
    }

    #[test]
    fn shapes_the_pass_never_writes_stay() {
        let source = "fn inline<T: Clone>(x: T) {}\n\
                      fn both<T: Send>(x: T) where T: Clone {}\n\
                      fn default<T = u8>(x: T) where T: Clone {}\n\
                      fn generics_comma<T,>(x: T) where T: Clone {}\n\
                      fn where_comma<T>(x: T)\nwhere\n    T: Clone,\n{\n}\n\
                      fn higher<T>(x: T) where for<'a> T: Fn(&'a u8) {}\n\
                      fn bounded_twice<T>(x: T) where T: Clone + Clone {}\n";
        assert_eq!(sugar(source).unwrap(), source);
    }

    #[test]
    fn a_name_the_pass_reads_as_a_type_stays_a_generic() {
        // As sugar, `m: Meters: Clone` is an error: a bound on a concrete type.
        let source = "struct Meters;\nfn f<Meters>(m: Meters) where Meters: Clone {}\n\
                      use std::fmt::Debug as D;\nfn g<D>(d: D) where D: Clone {}\n\
                      fn h<String>(s: String) where String: Clone {}\n";
        assert_eq!(edits(source), 0);
        assert_eq!(sugar(source).unwrap(), source);
    }

    #[test]
    fn a_comment_in_the_removed_text_keeps_the_signature_and_one_in_the_bounds_moves() {
        let source = "fn f<T /* why */>(x: T) where T: Clone {}\n\
                      fn g<T>(x: T) where /* why */ T: Clone {}\n";
        assert_eq!(edits(source), 0);
        assert_sugar(
            "fn f<T>(x: T) where T: Clone /* why */ + Send {}\n",
            "fn f(x: T: Clone /* why */ + Send) {}\n",
        );
    }

    #[test]
    fn the_three_places_are_one_edit_kept_or_dropped_whole() {
        let source = "fn f<A, B>(a: A, b: B) where A: Clone, B: Send {}\n";
        assert_eq!(edits(source), 1);
        // `T` is already the impl's (rustc rejects the shadowing), so as sugar the pass would
        // only add the bound: the driver refuses the edit, and no part of it survives.
        let source = "impl<T> S<T> {\n    fn f<T>(x: T) where T: Clone {}\n}\n";
        assert_eq!(edits(source), 1);
        assert_eq!(sugar(source).unwrap(), source);
    }

    #[test]
    fn edits_in_the_body_still_apply() {
        assert_sugar(
            "fn f<T>(x: T) where T: Clone { let mut y = x.clone(); y = x; }\n",
            "fn f(x: T: Clone) { var y = x.clone(); y = x; }\n",
        );
    }
}
