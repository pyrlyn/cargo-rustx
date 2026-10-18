//! Types back to the type prefixes of `passes/prefix_types.rs`: `Rc<RefCell<T>>` to `rc T` and
//! `Weak<RefCell<T>>` to `weak T` (problem 16, T19.10), `Box<dyn Trait + Send + Sync>` to `~Trait`
//! (problem 04) and `impl Iterator<Item = T> + '_` to `iter T` (problem 09, T19.13).
//!
//! The pass emits `::std::rc::Rc<::core::cell::RefCell<T>>`,
//! `::std::boxed::Box<dyn T + ::core::marker::Send + ::core::marker::Sync>` and
//! `impl ::core::iter::Iterator<Item = T> + '_`; any spelling of these paths that `std_paths`
//! resolves to the same items matches, as in `rc_values`. Nothing else does: a turbofish
//! (`Rc::<RefCell<T>>`), a second argument, a comment between the outer tokens, other bounds or
//! another bound order (`dyn A + Sync + Send`, `+ 'static` after the markers), a missing marker
//! or a missing `+ '_`. Several traits or a lifetime before the markers become `~(A + B)`.
//!
//! `~` is never valid Rust, so the pass reads it anywhere and a boxed trait object converts in
//! every type position. The words are ordinary identifiers, which `prefix_types` reads only where
//! the tokens before them say a type follows, so the other types convert only there: a named
//! field, a parameter, a `let` or a closure parameter (after `:`), a generic argument or
//! `Item = ..` binding, a `type` alias, a return type (after `->`), and behind `&` or `*` in one
//! of those. Not at the start of a group or after `,` outside `<..>`
//! (tuple elements, tuple struct fields, array and slice elements, fn pointer and `Fn(..)`
//! parameters), and not after `as` or `for`. After `->` or another prefix the type may start with
//! `(`, `[`, `&` or `*`; elsewhere only with a name, since `rc (..)` there reads as a call. Each
//! type is checked by expanding it alone in a position of its kind, which also refuses types the
//! pass would read only in part (`dyn A + B`) or not as a type (a macro after `:`).
//!
//! A type holding another such type converts inside out (`rc Vec<rc T>`, `iter rc ~T`).
//!
//! `rc_borrows` tracks a parameter, `let` or closure parameter typed `rc T` or `&rc T` and borrows
//! on its field accesses and value-method calls (problem 16), which changes plain code such as
//! `n.replace(v)`. Such a binding keeps its type: the rule asks the pass's own scan rather than
//! leave the edit to the driver's check. `weak T` is never tracked.

use std::collections::HashSet;

use proc_macro2::{Punct, Spacing, Span, TokenStream};
use quote::{ToTokens, quote};
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{
    AngleBracketedGenericArguments, Field, FnArg, GenericArgument, ImplItemFn, ImplItemType,
    ItemFn, ItemType, Local, Pat, PatIdent, PatType, Path, PathArguments, ReturnType, Signature,
    Stmt, Token, TraitItemFn, Type, TypeImplTrait, TypeParamBound, TypePath, TypeTraitObject,
};

use super::bounds;
use super::rc_values::apply_inside;
use super::std_paths::StdPaths;
use super::{Edit, ReverseRule};

pub(crate) struct PrefixTypes;

impl ReverseRule for PrefixTypes {
    fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit> {
        let paths = StdPaths::of(file);
        let shapes = Shapes {
            rc: paths.normalise(quote!(::std::rc::Rc)),
            weak: paths.normalise(quote!(::std::rc::Weak)),
            ref_cell: paths.normalise(quote!(::std::cell::RefCell)),
            boxed: paths.normalise(quote!(::std::boxed::Box)),
            send: paths.normalise(quote!(::core::marker::Send)),
            sync: paths.normalise(quote!(::core::marker::Sync)),
            iterator: paths.normalise(quote!(::core::iter::Iterator)),
            paths,
        };
        let mut finder = Finder::new(source, &shapes);
        finder.visit_file(file);
        finder.edits
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Prefix {
    Boxed,
    Iter,
    Rc,
    Weak,
}

impl Prefix {
    fn word(self) -> &'static str {
        match self {
            Self::Boxed => "~",
            Self::Iter => "iter",
            Self::Rc => "rc",
            Self::Weak => "weak",
        }
    }

    /// What the pass writes for this prefix before `inner`.
    fn plain(self, inner: &TokenStream) -> TokenStream {
        match self {
            Self::Boxed => {
                quote!(::std::boxed::Box<dyn #inner + ::core::marker::Send + ::core::marker::Sync>)
            }
            Self::Iter => quote!(impl ::core::iter::Iterator<Item = #inner> + '_),
            Self::Rc => quote!(::std::rc::Rc<::core::cell::RefCell<#inner>>),
            Self::Weak => quote!(::std::rc::Weak<::core::cell::RefCell<#inner>>),
        }
    }
}

/// Where a type sits, as `prefix_types` judges it from the tokens before it.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Position {
    /// Where the pass reads no word prefix, only `~`.
    Other,
    /// After `:`, `<`, `,` inside `<..>`, `=` or behind `&` or `*`: a prefix there is sugar only
    /// before a name.
    Name,
    /// After `->` or another prefix, where `(`, `[`, `&` and `*` may follow as well.
    Strong,
}

/// What follows a prefix: a type, or the bounds `~` puts before the markers.
enum Inner<'t> {
    Type(&'t Type),
    Bounds(Vec<&'t TypeParamBound>),
}

impl Inner<'_> {
    fn tokens(&self) -> TokenStream {
        match self {
            Self::Type(ty) => ty.to_token_stream(),
            Self::Bounds(bounds) => quote!(#(#bounds)+*),
        }
    }

    /// `~A` reads one trait bound; anything else needs `~(..)`.
    fn grouped(&self) -> bool {
        matches!(self, Self::Bounds(bounds) if !matches!(bounds[..], [TypeParamBound::Trait(_)]))
    }
}

/// A generated shape: its prefix, what follows the prefix and the last token of the shape.
struct Found<'t> {
    prefix: Prefix,
    inner: Inner<'t>,
    last: Span,
}

/// The normalised paths the rule looks for in one file.
struct Shapes {
    paths: StdPaths,
    rc: Vec<String>,
    weak: Vec<String>,
    ref_cell: Vec<String>,
    boxed: Vec<String>,
    send: Vec<String>,
    sync: Vec<String>,
    iterator: Vec<String>,
}

impl Shapes {
    fn find<'t>(&self, ty: &'t Type) -> Option<Found<'t>> {
        if let Type::ImplTrait(TypeImplTrait { attrs, bounds, .. }) = ty {
            let (inner, last) = self.iterator(bounds).filter(|_| attrs.is_empty())?;
            return Some(Found {
                prefix: Prefix::Iter,
                inner: Inner::Type(inner),
                last,
            });
        }
        let (path, arg, last) = self.type_argument(ty)?;
        if path == self.boxed {
            let bounds = self.boxed_bounds(arg)?;
            return Some(Found {
                prefix: Prefix::Boxed,
                inner: Inner::Bounds(bounds),
                last,
            });
        }
        let prefix = if path == self.rc {
            Prefix::Rc
        } else if path == self.weak {
            Prefix::Weak
        } else {
            return None;
        };
        let (path, inner, _) = self.type_argument(arg)?;
        (path == self.ref_cell).then_some(Found {
            prefix,
            inner: Inner::Type(inner),
            last,
        })
    }

    /// The bounds before the markers of `dyn .. + Send + Sync`.
    fn boxed_bounds<'t>(&self, ty: &'t Type) -> Option<Vec<&'t TypeParamBound>> {
        let Type::TraitObject(TypeTraitObject {
            attrs,
            dyn_token: Some(_),
            bounds,
        }) = ty
        else {
            return None;
        };
        let all: Vec<&TypeParamBound> = bounds.iter().collect();
        let [inner @ .., send, sync] = &all[..] else {
            return None;
        };
        let marker = |bound: &&TypeParamBound, path: &[String]| {
            self.paths.normalise(bound.to_token_stream()) == path
        };
        let plain = attrs.is_empty()
            && !inner.is_empty()
            && !bounds.trailing_punct()
            && marker(send, &self.send)
            && marker(sync, &self.sync);
        plain.then(|| inner.to_vec())
    }

    /// The `T` of `Iterator<Item = T> + '_`, with the `'_`.
    fn iterator<'t>(
        &self,
        bounds: &'t Punctuated<TypeParamBound, Token![+]>,
    ) -> Option<(&'t Type, Span)> {
        let mut all = bounds.iter();
        let (
            Some(TypeParamBound::Trait(trait_bound)),
            Some(TypeParamBound::Lifetime(lifetime)),
            None,
        ) = (all.next(), all.next(), all.next())
        else {
            return None;
        };
        // `?`, `for<..>`, parentheses and other modifiers print around the path.
        let bare = trait_bound.to_token_stream().to_string()
            == trait_bound.path.to_token_stream().to_string();
        let (path, arg, _) = self.single_argument(&trait_bound.path)?;
        let GenericArgument::AssocType(assoc) = arg else {
            return None;
        };
        let plain = bare
            && !bounds.trailing_punct()
            && lifetime.ident == "_"
            && path == self.iterator
            && assoc.ident == "Item"
            && assoc.generics.is_none();
        plain.then_some((&assoc.ty, lifetime.ident.span()))
    }

    /// [`Self::single_argument`] of a plain type path whose argument is a type.
    fn type_argument<'t>(&self, ty: &'t Type) -> Option<(Vec<String>, &'t Type, Span)> {
        let Type::Path(TypePath {
            qself: None,
            path,
            attrs,
        }) = ty
        else {
            return None;
        };
        match self.single_argument(path)? {
            (name, GenericArgument::Type(arg), close) if attrs.is_empty() => {
                Some((name, arg, close))
            }
            _ => None,
        }
    }

    /// The normalised path, its one generic argument and the `>` after it, when `path` has
    /// arguments only on its last segment and no turbofish.
    fn single_argument<'t>(
        &self,
        path: &'t Path,
    ) -> Option<(Vec<String>, &'t GenericArgument, Span)> {
        let last = path.segments.last()?;
        let PathArguments::AngleBracketed(args) = &last.arguments else {
            return None;
        };
        let plain = args.colon2_token.is_none()
            && args.args.len() == 1
            && !args.args.trailing_punct()
            && path
                .segments
                .iter()
                .rev()
                .skip(1)
                .all(|segment| segment.arguments.is_none());
        let arg = args.args.first().filter(|_| plain)?;
        let idents = path.segments.iter().map(|segment| &segment.ident);
        let colons = path.leading_colon;
        let name = self.paths.normalise(quote!(#colons #(#idents)::*));
        Some((name, arg, args.gt_token.spans[0]))
    }
}

struct Finder<'a> {
    source: &'a str,
    shapes: &'a Shapes,
    /// Start offsets of binding types that `rc_borrows` would track and borrow through.
    tracked: HashSet<usize>,
    edits: Vec<Edit>,
}

impl<'a> Finder<'a> {
    fn new(source: &'a str, shapes: &'a Shapes) -> Self {
        Self {
            source,
            shapes,
            tracked: HashSet::new(),
            edits: Vec::new(),
        }
    }

    /// Visits a type that sits where `prefix_types` reads a prefix, converting it if it matches.
    fn ty(&mut self, ty: &Type, position: Position) {
        match ty {
            Type::Reference(reference) => self.ty(&reference.elem, Position::Name),
            Type::Ptr(pointer) => self.ty(&pointer.elem, Position::Name),
            ty => match self.convert(ty, position) {
                Some(edit) => self.edits.push(edit),
                None => visit::visit_type(self, ty),
            },
        }
    }

    fn convert(&self, ty: &Type, position: Position) -> Option<Edit> {
        let found = self.shapes.find(ty)?;
        if position == Position::Other && found.prefix != Prefix::Boxed {
            return None;
        }
        let (first, _) = bounds(ty.to_token_stream())?;
        let inner = found.inner.tokens();
        let (inner_first, inner_last) = bounds(inner.clone())?;
        let (start, end) = (inner_first.byte_range().start, inner_last.byte_range().end);
        // Only path tokens and markers sit around the inner part, so a `/` there starts a
        // comment, which the edit would drop.
        let around = [
            first.byte_range().start..start,
            end..found.last.byte_range().end,
        ];
        let commented = around
            .into_iter()
            .any(|gap| self.source.get(gap).is_none_or(|gap| gap.contains('/')));
        let grouped = found.inner.grouped();
        if commented || !reads_back(found.prefix, &inner, grouped, position) {
            return None;
        }
        let mut nested = Finder::new(self.source, self.shapes);
        match &found.inner {
            Inner::Type(inner) => nested.ty(inner, Position::Strong),
            Inner::Bounds(bounds) => {
                for bound in bounds {
                    nested.visit_type_param_bound(bound);
                }
            }
        }
        let inner = apply_inside(self.source, start..end, nested.edits)?;
        let text = match (found.prefix, grouped) {
            (Prefix::Boxed, false) => format!("~{inner}"),
            (Prefix::Boxed, true) => format!("~({inner})"),
            (prefix, _) => format!("{} {inner}", prefix.word()),
        };
        Some(Edit::between(first, found.last, &text))
    }

    /// Records `typed` when `rc_borrows` would track it as an `rc` binding and borrow in `scope`.
    fn track(&mut self, typed: &PatType, scope: &[Stmt]) {
        let ty = match &*typed.ty {
            Type::Reference(reference) => &*reference.elem,
            ty => ty,
        };
        let Pat::Ident(PatIdent {
            ident,
            subpat: None,
            ..
        }) = &*typed.pat
        else {
            return;
        };
        let rc = matches!(
            self.shapes.find(ty),
            Some(Found {
                prefix: Prefix::Rc,
                ..
            })
        );
        if rc
            && crate::passes::would_borrow(ident, scope)
            && let Some((first, _)) = bounds(typed.ty.to_token_stream())
        {
            self.tracked.insert(first.byte_range().start);
        }
    }

    fn track_params(&mut self, sig: &Signature, body: &[Stmt]) {
        for input in &sig.inputs {
            if let FnArg::Typed(typed) = input {
                self.track(typed, body);
            }
        }
    }
}

/// Whether `prefix_types` reads the prefix before `inner`, in a position like `position`, as
/// exactly what it writes for that prefix.
fn reads_back(prefix: Prefix, inner: &TokenStream, grouped: bool, position: Position) -> bool {
    let sugar = match prefix {
        Prefix::Boxed => {
            let tilde = Punct::new('~', Spacing::Alone);
            if grouped {
                quote!(#tilde (#inner))
            } else {
                quote!(#tilde #inner)
            }
        }
        prefix => {
            let word = proc_macro2::Ident::new(prefix.word(), Span::call_site());
            quote!(#word #inner)
        }
    };
    let plain = prefix.plain(inner);
    let (sugar, plain) = match position {
        // After `:`, the strictest of these positions: it also refuses a macro. `~` reads the
        // same everywhere.
        Position::Name | Position::Other => (
            quote!(struct S { f: #sugar }),
            quote!(struct S { f: #plain }),
        ),
        Position::Strong => (quote!(fn f() -> #sugar {}), quote!(fn f() -> #plain {})),
    };
    let expanded = crate::try_expand(sugar).map(|file| file.into_token_stream().to_string());
    syn::parse2::<syn::File>(plain).is_ok_and(|plain| {
        expanded.is_ok_and(|expanded| expanded == plain.into_token_stream().to_string())
    })
}

impl<'ast> Visit<'ast> for Finder<'_> {
    /// A type in no position `prefix_types` reads a word in, where only `~` converts.
    fn visit_type(&mut self, ty: &'ast Type) {
        match self.convert(ty, Position::Other) {
            Some(edit) => self.edits.push(edit),
            None => visit::visit_type(self, ty),
        }
    }

    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        self.track_params(&item.sig, &item.block.stmts);
        visit::visit_item_fn(self, item);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        self.track_params(&item.sig, &item.block.stmts);
        visit::visit_impl_item_fn(self, item);
    }

    fn visit_trait_item_fn(&mut self, item: &'ast TraitItemFn) {
        if let Some(body) = &item.default {
            self.track_params(&item.sig, &body.stmts);
        }
        visit::visit_trait_item_fn(self, item);
    }

    fn visit_expr_closure(&mut self, closure: &'ast syn::ExprClosure) {
        if closure
            .inputs
            .iter()
            .any(|input| matches!(input, Pat::Type(_)))
        {
            // `rc_borrows` scans a closure body as one expression statement.
            let body = [Stmt::Expr((*closure.body).clone(), None)];
            for input in &closure.inputs {
                if let Pat::Type(typed) = input {
                    self.track(typed, &body);
                }
            }
        }
        visit::visit_expr_closure(self, closure);
    }

    fn visit_block(&mut self, block: &'ast syn::Block) {
        for (i, stmt) in block.stmts.iter().enumerate() {
            if let Stmt::Local(Local {
                pat: Pat::Type(typed),
                ..
            }) = stmt
            {
                self.track(typed, &block.stmts[i + 1..]);
            }
        }
        visit::visit_block(self, block);
    }

    fn visit_pat_type(&mut self, typed: &'ast PatType) {
        self.visit_pat(&typed.pat);
        let tracked = bounds(typed.ty.to_token_stream())
            .is_some_and(|(first, _)| self.tracked.contains(&first.byte_range().start));
        if tracked {
            // The tracked `Rc<RefCell<..>>` is not a candidate there, but types inside it are.
            visit::visit_type(self, &typed.ty);
        } else {
            self.ty(&typed.ty, Position::Name);
        }
    }

    fn visit_field(&mut self, field: &'ast Field) {
        // A tuple struct field starts a group or follows a `,` outside `<..>`.
        match field.colon_token {
            Some(_) => self.ty(&field.ty, Position::Name),
            None => visit::visit_field(self, field),
        }
    }

    fn visit_return_type(&mut self, ret: &'ast ReturnType) {
        if let ReturnType::Type(_, ty) = ret {
            self.ty(ty, Position::Strong);
        }
    }

    fn visit_angle_bracketed_generic_arguments(
        &mut self,
        args: &'ast AngleBracketedGenericArguments,
    ) {
        for arg in &args.args {
            match arg {
                GenericArgument::Type(ty) => self.ty(ty, Position::Name),
                // `Item<'a> = ..` puts a `>` before the `=`, where `prefix_types` sees no binding.
                GenericArgument::AssocType(assoc) if assoc.generics.is_none() => {
                    self.ty(&assoc.ty, Position::Name);
                }
                arg => self.visit_generic_argument(arg),
            }
        }
    }

    fn visit_item_type(&mut self, item: &'ast ItemType) {
        self.visit_generics(&item.generics);
        self.ty(&item.ty, Position::Name);
    }

    fn visit_impl_item_type(&mut self, item: &'ast ImplItemType) {
        self.visit_generics(&item.generics);
        self.ty(&item.ty, Position::Name);
    }
}

#[cfg(test)]
mod tests {
    use super::PrefixTypes;
    use crate::sugar::tests::assert_sugar;
    use crate::sugar::{Edit, ReverseRule, convert, sugar};

    const USES: &str = "use std::cell::RefCell;\nuse std::rc::{Rc, Weak};\n";

    fn with_uses(body: &str) -> String {
        format!("{USES}{body}")
    }

    /// Turns the first `Rc<RefCell<u8>>` into `rc u8` without looking at where it is.
    struct FirstRcU8;

    impl ReverseRule for FirstRcU8 {
        fn edits(&self, _: &syn::File, source: &str) -> Vec<Edit> {
            let at = source.find("Rc<RefCell<u8>>").unwrap();
            vec![Edit {
                range: at..at + "Rc<RefCell<u8>>".len(),
                text: "rc u8".to_owned(),
            }]
        }
    }

    /// Asserts the rule proposes nothing for `source` and the file comes back unchanged.
    fn stays_plain(source: &str) {
        let file = syn::parse_file(source).unwrap();
        assert_eq!(PrefixTypes.edits(&file, source), []);
        assert_eq!(sugar(source).unwrap(), source);
    }

    #[test]
    fn fields_returns_parameters_generic_arguments_and_aliases_convert() {
        assert_sugar(
            &with_uses(
                "struct Node {\n    parent: Option<Weak<RefCell<Node>>>,\n    \
                 children: Vec<Rc<RefCell<Node>>>,\n    by_id: HashMap<u32, Rc<RefCell<Node>>>,\n}\n\
                 type Link = Rc<RefCell<Node>>;\n\
                 impl Iterator for It {\n    type Item = Rc<RefCell<Node>>;\n    \
                 fn next(&mut self) -> Option<Self::Item> { None }\n}\n\
                 fn size(node: &Rc<RefCell<Node>>, up: Weak<RefCell<Node>>) -> Option<Rc<RefCell<Node>>> {\n    \
                 let _ = node.borrow().children.len();\n    up.upgrade()\n}\n\
                 fn all(v: &[Link]) -> impl Iterator<Item = Rc<RefCell<Node>>> + '_ {\n    \
                 let f = |n: Rc<RefCell<Node>>| -> Rc<RefCell<(u8, u8)>> { drop(n); g() };\n    \
                 let x: Rc<RefCell<u8>> = Rc::new(RefCell::new(1));\n    v.iter().cloned()\n}\n",
            ),
            &with_uses(
                "struct Node {\n    parent: Option<weak Node>,\n    \
                 children: Vec<rc Node>,\n    by_id: HashMap<u32, rc Node>,\n}\n\
                 type Link = rc Node;\n\
                 impl Iterator for It {\n    type Item = rc Node;\n    \
                 fn next(&mut self) -> Option<Self::Item> { None }\n}\n\
                 fn size(node: &rc Node, up: weak Node) -> Option<rc Node> {\n    \
                 let _ = node.borrow().children.len();\n    up.upgrade()\n}\n\
                 fn all(v: &[Link]) -> iter rc Node {\n    \
                 let f = |n: rc Node| -> rc (u8, u8) { drop(n); g() };\n    \
                 let x: rc u8 = rc 1;\n    v.iter().cloned()\n}\n",
            ),
        );
    }

    #[test]
    fn every_equivalent_spelling_converts_and_nested_types_convert_inside_out() {
        assert_sugar(
            "use std::rc;\nstruct S {\n    a: rc::Rc<std::cell::RefCell<u8>>,\n    \
             b: ::std::rc::Weak<::core::cell::RefCell<u8>>,\n    \
             c: std::rc::Rc<std::cell::RefCell<Vec<rc::Rc<std::cell::RefCell<u8>>>>>,\n    \
             d: rc::Rc<std::cell::RefCell<rc::Rc<std::cell::RefCell<u8>>>>,\n}\n",
            "use std::rc;\nstruct S {\n    a: rc u8,\n    b: weak u8,\n    \
             c: rc Vec<rc u8>,\n    d: rc rc u8,\n}\n",
        );
    }

    #[test]
    fn an_rc_binding_the_borrows_pass_would_rewrite_keeps_its_type() {
        // As sugar, `rc_borrows` would read `n.replace(2)` as `n.borrow_mut().replace(2)`.
        let source = with_uses(
            "fn f(n: Rc<RefCell<u8>>) -> u8 { n.replace(2) }\n\
             fn g() { let x: &Rc<RefCell<Vec<u8>>> = &k(); x.take(); }\n\
             fn h() { let c = |n: Rc<RefCell<u8>>| n.take(); }\n",
        );
        stays_plain(&source);
        // The driver's check refuses the edit as well, should a rule propose it.
        assert_eq!(convert(&source, &[&FirstRcU8]).unwrap(), source);
    }

    #[test]
    fn a_shadowed_rc_ref_cell_or_weak_stays_plain() {
        stays_plain("use my::Rc;\nuse std::cell::RefCell;\nstruct S { a: Rc<RefCell<u8>> }\n");
        stays_plain(&with_uses(
            "struct RefCell<T>(T);\nstruct S { a: Rc<RefCell<u8>> }\n",
        ));
        stays_plain(&with_uses("struct S<Weak> { a: Weak<RefCell<u8>> }\n"));
        stays_plain("struct S { a: rc::Rc<cell::RefCell<u8>> }\n");
    }

    #[test]
    fn a_type_at_a_group_start_or_after_a_comma_outside_generics_stays_plain() {
        stays_plain(&with_uses(
            "struct T(Rc<RefCell<u8>>, Weak<RefCell<u8>>);\n\
             fn f(t: (Rc<RefCell<u8>>, u8), a: [Rc<RefCell<u8>>; 2], s: &[Weak<RefCell<u8>>],\n    \
             p: fn(u8, Rc<RefCell<u8>>), b: Box<dyn Fn(Rc<RefCell<u8>>)>, q: (Rc<RefCell<u8>>)) {}\n\
             impl Tr for Rc<RefCell<u8>> {}\n\
             fn g(x: X) { let _ = x as Rc<RefCell<u8>>; }\n",
        ));
    }

    #[test]
    fn shapes_a_prefix_cannot_write_stay_plain() {
        stays_plain(&with_uses(
            "struct S {\n    a: Rc<RefCell<dyn A + B>>,\n    b: Rc<RefCell<::m::T>>,\n    \
             c: Rc<RefCell<(u8, u8)>>,\n    d: Rc</* why */ RefCell<u8>>,\n    \
             e: Rc<RefCell<u8> // why\n    >,\n    f: Rc::<RefCell<u8>>,\n    g: Rc<RefCell<u8>, A>,\n    \
             h: Rc<RefCell<m!()>>,\n    i: Rc<RefCell<[u8]>>,\n    j: Rc<RefCell<&'static u8>>,\n    \
             k: Rc<RefCell<<T as X>::Y>>,\n    l: Rc<RefCell<u8,>>,\n    m: Rc<RefCell<u8>>::Out,\n}\n",
        ));
    }

    #[test]
    fn layout_inside_the_inner_type_survives() {
        assert_sugar(
            &with_uses(
                "struct S {\n    a: Rc<RefCell<HashMap<\n        u8, // key\n        u8,\n    >>>,\n}\n",
            ),
            &with_uses(
                "struct S {\n    a: rc HashMap<\n        u8, // key\n        u8,\n    >,\n}\n",
            ),
        );
    }

    #[test]
    fn boxed_trait_objects_convert_in_every_type_position() {
        assert_sugar(
            "use std::fmt::Debug;\n\
             struct Bus {\n    sinks: Vec<Box<dyn Sink + Send + Sync>>,\n    \
             on: HashMap<String, Box<dyn Fn(&Event) -> bool + Send + Sync>>,\n}\n\
             struct Wrap(Box<dyn Sink + Send + Sync>, u8);\n\
             type Logger = Box<dyn Sink + Debug + Send + Sync>;\n\
             fn add(s: Box<dyn Sink + 'static + Send + Sync>, p: fn(Box<dyn Sink + Send + Sync>))\n    \
             -> Box<dyn Sink + Send + Sync> {\n    \
             let t: (Box<dyn Sink + Send + Sync>, u8) = (s, 0);\n    \
             t.0 as Box<dyn Sink + Send + Sync>\n}\n",
            "use std::fmt::Debug;\n\
             struct Bus {\n    sinks: Vec<~Sink>,\n    on: HashMap<String, ~Fn(&Event) -> bool>,\n}\n\
             struct Wrap(~Sink, u8);\n\
             type Logger = ~(Sink + Debug);\n\
             fn add(s: ~(Sink + 'static), p: fn(~Sink))\n    -> ~Sink {\n    \
             let t: (~Sink, u8) = (s, 0);\n    t.0 as ~Sink\n}\n",
        );
    }

    #[test]
    fn iterator_types_convert_where_the_pass_reads_a_prefix() {
        assert_sugar(
            "fn evens(v: &[i32]) -> impl Iterator<Item = &i32> + '_ { v.iter() }\n\
             fn pairs(v: &[i32]) -> impl Iterator<Item = (usize, &i32)> + '_ { v.iter().enumerate() }\n\
             fn sum(it: impl Iterator<Item = u8> + '_) -> u8 { it.sum() }\n",
            "fn evens(v: &[i32]) -> iter &i32 { v.iter() }\n\
             fn pairs(v: &[i32]) -> iter (usize, &i32) { v.iter().enumerate() }\n\
             fn sum(it: iter u8) -> u8 { it.sum() }\n",
        );
    }

    #[test]
    fn equivalent_spellings_of_box_iterator_and_the_markers_convert_and_nest() {
        assert_sugar(
            &with_uses(
                "struct S {\n    a: std::boxed::Box<dyn A + std::marker::Send + core::marker::Sync>,\n    \
                 b: Vec<Box<dyn Fn(Box<dyn A + Send + Sync>) + Send + Sync>>,\n}\n\
                 fn f(v: &[u8]) -> impl core::iter::Iterator<Item = Rc<RefCell<Box<dyn A + Send + Sync>>>> + '_ {\n    \
                 g(v)\n}\n",
            ),
            &with_uses(
                "struct S {\n    a: ~A,\n    b: Vec<~Fn(~A)>,\n}\n\
                 fn f(v: &[u8]) -> iter rc ~A {\n    g(v)\n}\n",
            ),
        );
    }

    #[test]
    fn boxed_shapes_the_pass_does_not_write_stay_plain() {
        stays_plain(
            "struct S {\n    a: Box<dyn A>,\n    b: Box<dyn A + Send>,\n    \
             c: Box<dyn A + Sync + Send>,\n    d: Box<dyn Send + Sync>,\n    \
             e: Box<dyn A + Send + Sync + 'static>,\n    f: Box<dyn A + Send + Sync, G>,\n    \
             g: Box::<dyn A + Send + Sync>,\n    h: Box<dyn (A) + Send + Sync>,\n    \
             i: Box</* why */ dyn A + Send + Sync>,\n    j: Box<dyn A + Send + Sync /* why */>,\n    \
             k: Box<A + Send + Sync>,\n    l: Box<dyn A + Send + (Sync)>,\n    \
             m: Box<dyn A + Send + Sync<u8>>,\n}\n",
        );
    }

    #[test]
    fn iterator_shapes_the_pass_does_not_write_or_read_stay_plain() {
        stays_plain(
            "fn a(v: V) -> impl Iterator<Item = u8> { g(v) }\n\
             fn b(v: V) -> impl Iterator<Item = u8> + 'static { g(v) }\n\
             fn c(v: V) -> impl '_ + Iterator<Item = u8> { g(v) }\n\
             fn d(v: V) -> impl Iterator<Item = u8> + '_ + Send { g(v) }\n\
             fn e(v: V) -> impl Iterator<Item = u8,> + '_ { g(v) }\n\
             fn f(v: V) -> impl DoubleEndedIterator<Item = u8> + '_ { g(v) }\n\
             fn g(v: V) -> impl Iterator<Item = dyn A + B> + '_ { g(v) }\n\
             fn h(v: V) -> impl Iterator<Item = u8> /* why */ + '_ { g(v) }\n\
             fn i(v: V) -> impl for<'a> Iterator<Item = u8> + '_ { g(v) }\n\
             fn j(v: V) -> impl Iterator::<Item = u8> + '_ { g(v) }\n\
             fn k(v: V) -> (impl Iterator<Item = u8> + '_, u8) { g(v) }\n\
             fn l(v: V) -> &(impl Iterator<Item = u8> + '_) { g(v) }\n",
        );
    }

    #[test]
    fn a_shadowed_box_iterator_send_or_sync_stays_plain() {
        stays_plain("struct Box<T>(T);\nstruct S { a: Box<dyn A + Send + Sync> }\n");
        stays_plain("use my::Iterator;\nfn f(v: V) -> impl Iterator<Item = u8> + '_ { g(v) }\n");
        stays_plain("trait Send {}\nstruct S { a: Box<dyn A + Send + Sync> }\n");
        stays_plain("struct S<Sync> { a: Box<dyn A + Send + Sync>, b: Sync }\n");
    }

    #[test]
    fn references_to_prefixed_types_and_trait_methods_convert() {
        assert_sugar(
            &with_uses(
                "trait Tree {\n    fn root(&self) -> Rc<RefCell<u8>>;\n    \
                 fn up(&self, to: &mut Weak<RefCell<u8>>) -> Option<Weak<RefCell<u8>>>;\n}\n",
            ),
            &with_uses(
                "trait Tree {\n    fn root(&self) -> rc u8;\n    \
                 fn up(&self, to: &mut weak u8) -> Option<weak u8>;\n}\n",
            ),
        );
    }

    #[test]
    fn a_box_and_an_iterator_in_the_same_signature_both_convert() {
        assert_sugar(
            "fn run(h: &[Box<dyn Job + Send + Sync>]) -> impl Iterator<Item = Box<dyn Job + Send + Sync>> + '_ {\n    \
             h.iter().map(|_| todo!())\n}\n",
            "fn run(h: &[~Job]) -> iter ~Job {\n    h.iter().map(|_| todo!())\n}\n",
        );
    }

    #[test]
    fn several_traits_and_a_lifetime_before_the_markers_become_a_group() {
        assert_sugar(
            "struct S {\n    a: Box<dyn A + B + Send + Sync>,\n    \
             b: Box<dyn A + 'static + Send + Sync>,\n}\n",
            "struct S {\n    a: ~(A + B),\n    b: ~(A + 'static),\n}\n",
        );
    }

    #[test]
    fn source_that_does_not_parse_is_an_error_not_a_panic() {
        assert!(sugar("struct S { a: Rc<RefCell<u8> }").is_err());
        assert!(sugar("fn f() -> impl Iterator<Item = u8 + '_ {").is_err());
    }

    #[test]
    fn a_file_without_any_prefix_shape_is_returned_byte_for_byte() {
        stays_plain("// nothing here\nstruct S { a: Vec<u8>, b: Option<String> }\n");
    }
}
