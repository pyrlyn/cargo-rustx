//! `x: impl Into<T>` with its conversion back to `x: into T`: the reverse of
//! `passes/into_params.rs` (problem 15, T19.6).
//!
//! For `x: into T` the pass writes the type `impl ::core::convert::Into<T>` and starts the body
//! with `let x: T = ::core::convert::Into::into(x);`, the conversions of all such parameters
//! first and in parameter order. The parameter's `mut` or `ref` moves to the `let`, and its
//! attributes are copied onto it. A parameter converts only when the body starts with exactly
//! that rebinding (after the rebindings of the parameters before it), compared as tokens with
//! std paths resolved by `std_paths`, so `Into` and `Into::into` may be spelled short.
//!
//! Everything else stays as written, because nothing shows it came from the sugar: an
//! `impl Into<T>` used without that rebinding, a rebinding that calls `.into()`, renames, retypes
//! or comes after other code, a generic `S: Into<T>`, and a declaration without a body (the pass
//! gives a trait method only the type, which is also how plain Rust writes it). So does a `T`
//! that starts with `<` or `::`, which `into` cannot be followed by, and a parameter whose type
//! or rebinding holds a comment the rewrite would drop.
//!
//! The parameter and its rebinding are two places in the text but one edit, spanning from the
//! first converted parameter of a fn to its last removed rebinding. Either half alone does not
//! expand back (the conversion would run twice, or not at all), so as two edits the driver's
//! one-at-a-time fallback would drop both; as one, a fn's conversions are kept or dropped
//! together. Another rule's edit inside that span, such as `var` on a `let mut` rebinding, is
//! dropped by the driver; the rebinding it would rewrite is removed anyway.

use std::ops::Range;

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::visit::{self, Visit};
use syn::{
    Block, FnArg, GenericArgument, ImplItemFn, ItemFn, Pat, PatIdent, PatType, PathArguments,
    Signature, Stmt, TraitItemFn, Type, TypeImplTrait, TypeParamBound,
};

use super::std_paths::StdPaths;
use super::{Edit, ReverseRule, apply, bounds, leaves};

pub(crate) struct IntoParams;

impl ReverseRule for IntoParams {
    fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit> {
        let paths = StdPaths::of(file);
        let mut finder = Finder {
            source,
            paths: &paths,
            edits: Vec::new(),
        };
        finder.visit_file(file);
        finder.edits
    }
}

struct Finder<'a> {
    source: &'a str,
    paths: &'a StdPaths,
    edits: Vec<Edit>,
}

impl Finder<'_> {
    /// One edit for all converted parameters of a fn with a body, if any converts.
    fn convert(&mut self, sig: &Signature, body: &Block) {
        let mut stmts = body.stmts.iter();
        let mut next = stmts.next();
        let mut parts = Vec::new();
        for arg in &sig.inputs {
            let (FnArg::Typed(param), Some(stmt)) = (arg, next) else {
                continue;
            };
            if let Some(found) = self.parameter(param, stmt) {
                parts.extend(found);
                next = stmts.next();
            }
        }
        parts.sort_by_key(|part| part.range.start);
        let (Some(first), Some(last)) = (parts.first(), parts.last()) else {
            return;
        };
        let range = first.range.start..last.range.end;
        let Some(text) = self.source.get(range.clone()) else {
            return;
        };
        let relative: Vec<Edit> = parts
            .iter()
            .map(|part| Edit {
                range: part.range.start - range.start..part.range.end - range.start,
                text: part.text.clone(),
            })
            .collect();
        let text = apply(text, &relative);
        self.edits.push(Edit { range, text });
    }

    /// The edit turning `param` into `x: into T` and the one removing `stmt`, when `stmt` is
    /// exactly the rebinding the pass writes for that parameter.
    fn parameter(&self, param: &PatType, stmt: &Stmt) -> Option<[Edit; 2]> {
        let Pat::Ident(PatIdent {
            by_ref: None,
            mutability: None,
            subpat: None,
            ident,
            ..
        }) = &*param.pat
        else {
            return None;
        };
        let Type::ImplTrait(impl_trait) = &*param.ty else {
            return None;
        };
        let target = into_target(impl_trait).filter(|target| starts_a_type(target))?;
        let Stmt::Local(local) = stmt else {
            return None;
        };
        let Pat::Type(typed) = &local.pat else {
            return None;
        };
        let Pat::Ident(PatIdent {
            by_ref, mutability, ..
        }) = &*typed.pat
        else {
            return None;
        };

        // What the pass writes for `x: into T` with this binding mode, compared as tokens, so
        // every other modifier, bound or attribute on either side refuses.
        let attrs = &param.attrs;
        let ty = quote!(impl ::core::convert::Into<#target>);
        let rebinding = quote! {
            #(#attrs)* let #by_ref #mutability #ident: #target = ::core::convert::Into::into(#ident);
        };
        let same = |ours: TokenStream, theirs: TokenStream| {
            self.paths.normalise(ours) == self.paths.normalise(theirs)
        };
        if !same(ty, param.ty.to_token_stream()) || !same(rebinding, stmt.to_token_stream()) {
            return None;
        }

        let target = byte_range(target.to_token_stream())?;
        let ty = byte_range(param.ty.to_token_stream())?;
        let stmt_tokens = stmt.to_token_stream();
        let stmt = byte_range(stmt_tokens.clone())?;
        // Text outside `T` is regenerated or deleted, so a comment there would be lost.
        if !self.blank_gaps(param.ty.to_token_stream(), &target)
            || !self.blank_gaps(stmt_tokens, &(0..0))
        {
            return None;
        }

        let name = ident.span().byte_range().start;
        let impl_start = impl_trait.impl_token.span.byte_range().start;
        let mode = format!(
            "{}{}",
            if by_ref.is_some() { "ref " } else { "" },
            if mutability.is_some() { "mut " } else { "" }
        );
        let into = format!(
            "{mode}{}into {}",
            self.source.get(name..impl_start)?,
            self.source.get(target)?
        );
        Some([
            Edit {
                range: name..ty.end,
                text: into,
            },
            Edit {
                range: self.removal(stmt),
                text: String::new(),
            },
        ])
    }

    /// Whether every gap between two tokens of `tokens` is whitespace, apart from gaps inside
    /// `keep`, whose text is copied rather than dropped.
    fn blank_gaps(&self, tokens: TokenStream, keep: &Range<usize>) -> bool {
        let mut all = Vec::new();
        leaves(tokens, &mut all);
        all.windows(2).all(|pair| {
            let gap = pair[0].1.byte_range().end..pair[1].1.byte_range().start;
            (keep.start <= gap.start && gap.end <= keep.end)
                || self
                    .source
                    .get(gap)
                    .is_some_and(|gap| gap.trim().is_empty())
        })
    }

    /// The bytes to delete for a removed statement: its whole line when nothing else is on it,
    /// otherwise the statement and the blanks after it, and before it at the end of a line.
    fn removal(&self, stmt: Range<usize>) -> Range<usize> {
        let source = self.source;
        let line_start = source[..stmt.start].rfind('\n').map_or(0, |at| at + 1);
        let line_end = source[stmt.end..]
            .find('\n')
            .map_or(source.len(), |at| stmt.end + at);
        let horizontal = |c: char| c == ' ' || c == '\t';
        let after = source.len() - source[stmt.end..].trim_start_matches(horizontal).len();
        if !source[stmt.end..line_end].trim().is_empty() {
            return stmt.start..after;
        }
        if source[line_start..stmt.start].trim().is_empty() {
            return line_start..(line_end + 1).min(source.len());
        }
        let before = source[..stmt.start].trim_end_matches(horizontal).len();
        before..line_end
    }
}

/// The bytes from the first to the last token of `tokens`.
fn byte_range(tokens: TokenStream) -> Option<Range<usize>> {
    let (first, last) = bounds(tokens)?;
    Some(first.byte_range().start..last.byte_range().end)
}

/// The `T` of `impl Into<T>` with one bound and one type argument; the token comparison decides
/// whether the rest is exactly that.
fn into_target(impl_trait: &TypeImplTrait) -> Option<&Type> {
    let Some(TypeParamBound::Trait(bound)) = impl_trait.bounds.first() else {
        return None;
    };
    let PathArguments::AngleBracketed(args) = &bound.path.segments.last()?.arguments else {
        return None;
    };
    match args.args.first()? {
        GenericArgument::Type(target) => Some(target),
        _ => None,
    }
}

/// Whether `into` before `target` reads as the sugar: the marker pass needs a type that starts
/// with a name, `&`, `*`, `(..)` or `[..]`, so `<T as Tr>::A` and `::a::B` cannot follow it.
fn starts_a_type(target: &Type) -> bool {
    let mut all = Vec::new();
    leaves(target.to_token_stream(), &mut all);
    all.first().is_some_and(|(token, _)| {
        token.starts_with(|c: char| c.is_alphabetic() || c == '_')
            || ["&", "*", "(", "["].contains(&token.as_str())
    })
}

impl<'ast> Visit<'ast> for Finder<'_> {
    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        self.convert(&item.sig, &item.block);
        visit::visit_item_fn(self, item);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        self.convert(&item.sig, &item.block);
        visit::visit_impl_item_fn(self, item);
    }

    fn visit_trait_item_fn(&mut self, item: &'ast TraitItemFn) {
        if let Some(body) = &item.default {
            self.convert(&item.sig, body);
        }
        visit::visit_trait_item_fn(self, item);
    }
}

#[cfg(test)]
mod tests {
    use super::IntoParams;
    use crate::sugar::tests::assert_sugar;
    use crate::sugar::{Edit, ReverseRule, convert, sugar};

    /// The rule proposes nothing for `source`, and the driver returns it byte for byte.
    fn stays_plain(source: &str) {
        let file = syn::parse_file(source).unwrap();
        assert_eq!(IntoParams.edits(&file, source), [], "{source}");
        assert_eq!(sugar(source).unwrap(), source);
    }

    /// Replaces the first occurrence of a text, whether or not that keeps the meaning.
    struct Replace(&'static str, &'static str);

    impl ReverseRule for Replace {
        fn edits(&self, _: &syn::File, source: &str) -> Vec<Edit> {
            source
                .find(self.0)
                .map(|at| Edit {
                    range: at..at + self.0.len(),
                    text: self.1.to_owned(),
                })
                .into_iter()
                .collect()
        }
    }

    const NEW: &str = "fn new(name: impl Into<String>) -> S {\n    \
                       let name: String = Into::into(name);\n    S { name }\n}\n";

    #[test]
    fn an_into_parameter_and_its_conversion_become_into_sugar() {
        assert_sugar(NEW, "fn new(name: into String) -> S {\n    S { name }\n}\n");
    }

    #[test]
    fn every_spelling_std_paths_equates_converts() {
        assert_sugar(
            "use std::convert;\n\
             fn a(x: impl ::core::convert::Into<u8>) { let x: u8 = ::core::convert::Into::into(x); }\n\
             fn b(x: impl std::convert::Into<Vec<u8>>) { let x: ::std::vec::Vec<u8> = Into::into(x); }\n\
             fn c(x: impl convert::Into<&'static str>) { let x: &'static str = convert::Into::into(x); }\n",
            "use std::convert;\n\
             fn a(x: into u8) { }\n\
             fn b(x: into Vec<u8>) { }\n\
             fn c(x: into &'static str) { }\n",
        );
    }

    #[test]
    fn parameters_convert_in_order_with_their_binding_mode_and_attributes() {
        assert_sugar(
            "impl S {\n    fn f(&mut self, #[cfg(unix)] a: impl Into<String>, n: u8, b: impl Into<(u8, u8)>, \
             c: impl Into<[u8; 2]>) {\n        \
             #[cfg(unix)]\n        let mut a: String = Into::into(a);\n        \
             let ref b: (u8, u8) = Into::into(b);\n        let c: [u8; 2] = Into::into(c);\n        \
             a.push('!');\n    }\n}\n",
            "impl S {\n    fn f(&mut self, #[cfg(unix)] mut a: into String, n: u8, ref b: into (u8, u8), \
             c: into [u8; 2]) {\n        a.push('!');\n    }\n}\n",
        );
    }

    #[test]
    fn a_let_mut_rebinding_becomes_mut_into_rather_than_var() {
        assert_sugar(
            "fn f(s: impl Into<String>) -> String {\n    let mut s: String = Into::into(s);\n    \
             s.push('!');\n    let mut t = s;\n    t\n}\n",
            "fn f(mut s: into String) -> String {\n    s.push('!');\n    var t = s;\n    t\n}\n",
        );
    }

    #[test]
    fn rebindings_anywhere_on_their_line_leave_tidy_text_and_comments_around_them_stay() {
        assert_sugar(
            "fn a(x: impl Into<u8>) -> u8 { let x: u8 = Into::into(x); x }\n\
             fn b(x: impl Into<u8>) -> u8 { let x: u8 = Into::into(x);\n    x\n}\n\
             fn c(x: impl Into<u8>) {\n    // why\n    let x: u8 = Into::into(x); // kept\n}\n\
             fn d(x: impl Into<u8>) {\n    let x: u8 = Into::into(x);\n}\n",
            "fn a(x: into u8) -> u8 { x }\n\
             fn b(x: into u8) -> u8 {\n    x\n}\n\
             fn c(x: into u8) {\n    // why\n    // kept\n}\n\
             fn d(x: into u8) {\n}\n",
        );
    }

    #[test]
    fn nested_fns_and_trait_default_methods_convert() {
        assert_sugar(
            "trait T {\n    fn d(x: impl Into<u32>) {\n        let x: u32 = Into::into(x);\n        \
             fn inner(y: impl Into<u8>) { let y: u8 = Into::into(y); }\n    }\n}\n",
            "trait T {\n    fn d(x: into u32) {\n        \
             fn inner(y: into u8) { }\n    }\n}\n",
        );
    }

    #[test]
    fn only_rebindings_at_the_top_in_parameter_order_convert() {
        // The pass would have written `b`'s conversion first only if `a` were not sugar.
        assert_sugar(
            "fn f(a: impl Into<u8>, b: impl Into<u8>) {\n    let b: u8 = Into::into(b);\n    \
             let a: u8 = Into::into(a);\n}\n",
            "fn f(a: impl Into<u8>, b: into u8) {\n    let a: u8 = Into::into(a);\n}\n",
        );
    }

    #[test]
    fn other_uses_of_impl_into_stay_plain() {
        for source in [
            // No rebinding, or one that is not first.
            "fn f(x: impl Into<String>) -> String { x.into() }\n",
            "fn f(x: impl Into<String>) { g(); let x: String = Into::into(x); }\n",
            // A rebinding the pass does not write.
            "fn f(x: impl Into<String>) { let x: String = x.into(); }\n",
            "fn f(x: impl Into<String>) { let y: String = Into::into(x); }\n",
            "fn f(x: impl Into<String>) { let x: Box<str> = Into::into(x); }\n",
            "fn f(x: impl Into<String>) { let x = Into::<String>::into(x); }\n",
            "fn f(x: impl Into<String>) { let x: String = Into::into(x) else { return }; }\n",
            "fn f(#[cfg(unix)] x: impl Into<String>) { let x: String = Into::into(x); }\n",
            "fn f(x: impl Into<String>) { #[allow(unused)] let x: String = Into::into(x); }\n",
            // A parameter the pass does not write.
            "fn f<S: Into<String>>(x: S) { let x: String = Into::into(x); }\n",
            "fn f(mut x: impl Into<String>) { let x: String = Into::into(x); }\n",
            "fn f(x: impl Into<String> + Send) { let x: String = Into::into(x); }\n",
            "fn f(x: impl Into<String,>) { let x: String = Into::into(x); }\n",
            "fn f(x: impl ?Sized + Into<String>) { let x: String = Into::into(x); }\n",
            "fn f(x: impl From<String>) { let x: String = From::from(x); }\n",
            // A declaration has no rebinding to show the type came from the sugar.
            "trait T { fn f(x: impl Into<String>); }\n",
            // `into` cannot be followed by a type starting with `::` or `<`.
            "fn f(x: impl Into<::std::string::String>) { let x: ::std::string::String = Into::into(x); }\n",
            "fn f(x: impl Into<<u8 as T>::A>) { let x: <u8 as T>::A = Into::into(x); }\n",
        ] {
            stays_plain(source);
        }
    }

    #[test]
    fn a_shadowed_into_stays_plain() {
        stays_plain("use my::Into;\nfn f(x: impl Into<u8>) { let x: u8 = Into::into(x); }\n");
        stays_plain("trait Into<T> {}\nfn f(x: impl Into<u8>) { let x: u8 = Into::into(x); }\n");
    }

    #[test]
    fn a_comment_the_rewrite_would_drop_keeps_the_parameter_plain() {
        stays_plain("fn f(x: impl /* c */ Into<u8>) { let x: u8 = Into::into(x); }\n");
        stays_plain("fn f(x: impl Into<u8>) { let x: u8 = /* c */ Into::into(x); }\n");
        // Inside `T` the text is copied, so the comment stays.
        assert_sugar(
            "fn f(x: impl Into<Vec</* c */ u8>>) { let x: Vec<u8> = Into::into(x); }\n",
            "fn f(x: into Vec</* c */ u8>) { }\n",
        );
    }

    #[test]
    fn either_half_of_a_conversion_alone_does_not_expand_back() {
        let type_only = Replace("impl Into<String>", "into String");
        assert_eq!(convert(NEW, &[&type_only]).unwrap(), NEW);
        let rebinding_only = Replace("let name: String = Into::into(name);", "");
        assert_eq!(convert(NEW, &[&rebinding_only]).unwrap(), NEW);
    }

    #[test]
    fn a_fn_converts_whole_when_a_bad_edit_forces_the_one_by_one_fallback() {
        let source = "fn f(a: impl Into<u8>, b: impl Into<u8>) {\n    let a: u8 = Into::into(a);\n    \
                      let b: u8 = Into::into(b);\n}\nfn g() {}\n";
        let rename = Replace("fn g", "fn h");
        assert_eq!(
            convert(source, &[&IntoParams, &rename]).unwrap(),
            "fn f(a: into u8, b: into u8) {\n}\nfn g() {}\n"
        );
    }
}
