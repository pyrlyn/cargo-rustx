//! `Rc<RefCell<T>>` back to `rc T` and `Weak<RefCell<T>>` back to `weak T`: the reverse of the
//! type half of `passes/prefix_types.rs` (problem 16, T19.10).
//!
//! The pass emits `::std::rc::Rc<::core::cell::RefCell<T>>`; any spelling of the two paths that
//! `std_paths` resolves to the same items matches, as in `rc_values`. A turbofish
//! (`Rc::<RefCell<T>>`), a second argument or a comment between the outer tokens does not.
//!
//! `prefix_types` reads a prefix only where the tokens before it say a type follows, so a type
//! converts only there: a named field, a parameter, a `let` or a closure parameter (after `:`),
//! a generic argument or `Item = ..` binding, a `type` alias, a return type (after `->`), and
//! behind `&` or `*` in one of those. Not at the start of a group or after `,` outside `<..>`
//! (tuple elements, tuple struct fields, array and slice elements, fn pointer and `Fn(..)`
//! parameters), and not after `as` or `for`. After `->` or another prefix the type may start with
//! `(`, `[`, `&` or `*`; elsewhere only with a name, since `rc (..)` there reads as a call. Each
//! type is checked by expanding it alone in a position of its kind, which also refuses types the
//! pass would read only in part (`dyn A + B`) or not as a type (a macro after `:`).
//!
//! A type holding another such type converts inside out (`rc Vec<rc T>`, `rc rc T`).
//!
//! `rc_borrows` tracks a parameter, `let` or closure parameter typed `rc T` or `&rc T` and borrows
//! on its field accesses and value-method calls (problem 16), which changes plain code such as
//! `n.replace(v)`. Such a binding keeps its type: the rule asks the pass's own scan rather than
//! leave the edit to the driver's check. `weak T` is never tracked.

use std::collections::HashSet;

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::visit::{self, Visit};
use syn::{
    AngleBracketedGenericArguments, Field, FnArg, GenericArgument, ImplItemFn, ImplItemType,
    ItemFn, ItemType, Local, Pat, PatIdent, PatType, PathArguments, ReturnType, Signature, Stmt,
    TraitItemFn, Type, TypePath,
};

use super::rc_values::{apply_inside, bounds};
use super::std_paths::StdPaths;
use super::{Edit, ReverseRule};

pub(crate) struct RcTypes;

impl ReverseRule for RcTypes {
    fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit> {
        let paths = StdPaths::of(file);
        let shapes = Shapes {
            rc: paths.normalise(quote!(::std::rc::Rc)),
            weak: paths.normalise(quote!(::std::rc::Weak)),
            ref_cell: paths.normalise(quote!(::std::cell::RefCell)),
            paths,
        };
        let mut finder = Finder::new(source, &shapes);
        finder.visit_file(file);
        finder.edits
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Prefix {
    Rc,
    Weak,
}

impl Prefix {
    fn word(self) -> &'static str {
        match self {
            Self::Rc => "rc",
            Self::Weak => "weak",
        }
    }

    fn pointer(self) -> TokenStream {
        match self {
            Self::Rc => quote!(::std::rc::Rc),
            Self::Weak => quote!(::std::rc::Weak),
        }
    }
}

/// Where a type sits, as `prefix_types` judges it from the tokens before it.
#[derive(Clone, Copy)]
enum Position {
    /// After `:`, `<`, `,` inside `<..>`, `=` or behind `&` or `*`: a prefix there is sugar only
    /// before a name.
    Name,
    /// After `->` or another prefix, where `(`, `[`, `&` and `*` may follow as well.
    Strong,
}

/// The normalised paths the rule looks for in one file.
struct Shapes {
    paths: StdPaths,
    rc: Vec<String>,
    weak: Vec<String>,
    ref_cell: Vec<String>,
}

impl Shapes {
    /// The prefix and inner type of `Rc<RefCell<T>>` or `Weak<RefCell<T>>`, with the `>` that
    /// closes the outer pointer.
    fn pointer<'t>(&self, ty: &'t Type) -> Option<(Prefix, &'t Type, Span)> {
        let (path, cell, close) = self.single_argument(ty)?;
        let prefix = if path == self.rc {
            Prefix::Rc
        } else if path == self.weak {
            Prefix::Weak
        } else {
            return None;
        };
        let (path, inner, _) = self.single_argument(cell)?;
        (path == self.ref_cell).then_some((prefix, inner, close))
    }

    /// The normalised path of `ty`, its one type argument and the `>` after it, when `ty` is a
    /// plain path with arguments only on its last segment and no turbofish.
    fn single_argument<'t>(&self, ty: &'t Type) -> Option<(Vec<String>, &'t Type, Span)> {
        let Type::Path(TypePath {
            qself: None,
            path,
            attrs,
        }) = ty
        else {
            return None;
        };
        let last = path.segments.last()?;
        let PathArguments::AngleBracketed(args) = &last.arguments else {
            return None;
        };
        let plain = attrs.is_empty()
            && args.colon2_token.is_none()
            && args.args.len() == 1
            && !args.args.trailing_punct()
            && path
                .segments
                .iter()
                .rev()
                .skip(1)
                .all(|segment| segment.arguments.is_none());
        let Some(GenericArgument::Type(arg)) = args.args.first().filter(|_| plain) else {
            return None;
        };
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
                None => self.visit_type(ty),
            },
        }
    }

    fn convert(&self, ty: &Type, position: Position) -> Option<Edit> {
        let (prefix, inner, close) = self.shapes.pointer(ty)?;
        let (first, _) = bounds(ty.to_token_stream())?;
        let (inner_first, inner_last) = bounds(inner.to_token_stream())?;
        let (start, end) = (inner_first.byte_range().start, inner_last.byte_range().end);
        // Only path tokens sit around the inner type, so a `/` there starts a comment, which the
        // edit would drop.
        let around = [first.byte_range().start..start, end..close.byte_range().end];
        let commented = around
            .into_iter()
            .any(|gap| self.source.get(gap).is_none_or(|gap| gap.contains('/')));
        if commented || !reads_back(prefix, inner, position) {
            return None;
        }
        let mut nested = Finder::new(self.source, self.shapes);
        nested.ty(inner, Position::Strong);
        let inner = apply_inside(self.source, start..end, nested.edits)?;
        Some(Edit::between(
            first,
            close,
            &format!("{} {inner}", prefix.word()),
        ))
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
        let rc = matches!(self.shapes.pointer(ty), Some((Prefix::Rc, ..)));
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

/// Whether `prefix_types` reads `prefix inner`, in a position like `position`, as exactly
/// `Rc<RefCell<inner>>` or `Weak<RefCell<inner>>`.
fn reads_back(prefix: Prefix, inner: &Type, position: Position) -> bool {
    let word = proc_macro2::Ident::new(prefix.word(), Span::call_site());
    let pointer = prefix.pointer();
    let plain = quote!(#pointer<::core::cell::RefCell<#inner>>);
    let (sugar, plain) = match position {
        // After `:`, the strictest of these positions: it also refuses a macro.
        Position::Name => (
            quote!(struct S { f: #word #inner }),
            quote!(struct S { f: #plain }),
        ),
        Position::Strong => (
            quote!(fn f() -> #word #inner {}),
            quote!(fn f() -> #plain {}),
        ),
    };
    let expanded = crate::try_expand(sugar).map(|file| file.into_token_stream().to_string());
    syn::parse2::<syn::File>(plain).is_ok_and(|plain| {
        expanded.is_ok_and(|expanded| expanded == plain.into_token_stream().to_string())
    })
}

impl<'ast> Visit<'ast> for Finder<'_> {
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
            self.visit_type(&typed.ty);
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
    use super::RcTypes;
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
        assert_eq!(RcTypes.edits(&file, source), []);
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
                 fn all(v: &[Link]) -> impl Iterator<Item = rc Node> + '_ {\n    \
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
}
