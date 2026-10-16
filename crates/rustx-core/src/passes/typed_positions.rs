//! Places where a value goes into a type the input writes, for the conversion passes (problems
//! 13 and 15), which have no type information of their own.
//!
//! A target type counts only when it is written: a `let` annotation, a local or a field of a struct
//! declared in the input (assigned or compound-assigned), a parameter of a fn declared in the
//! input, called by path (calls resolve by name, as call-site defaults do), and, for a pass that
//! asks for them, the fields of a literal of a declared struct. The type of a value is known only for a name bound
//! with a written type (a `let` annotation, a parameter, a closure parameter, the element of a
//! `for` loop over a slice, array or `Vec` of known type), a field of a declared struct reached
//! from one, and `*` of a known reference. Any other binding of a name ends what is known about
//! it, so a shadowing `let x = ..` never leaves a stale type behind.
//!
//! Struct and fn names are matched by their last path segment across the whole input, so the
//! types a conversion pass is given are guesses it must make safe: it should spell the conversion
//! so that a wrong guess fails to compile.

use std::collections::HashMap;
use std::mem;

use syn::spanned::Spanned;
use syn::visit_mut::{self, VisitMut};
use syn::{
    Arm, Attribute, BinOp, Block, Expr, ExprCall, ExprClosure, ExprForLoop, ExprIf, ExprLet,
    ExprStruct, ExprWhile, FnArg, ImplItemFn, Item, ItemFn, ItemImpl, ItemMod, ItemStruct, Local,
    Member, Pat, PatIdent, Path, ReceiverKind, Signature, Token, TraitItemFn, Type, TypeReference,
    UnOp,
};

use super::call_defaults::collect::{Fns, path_target, type_name};
use crate::tokens::syntax::std_argument;

/// A conversion pass: called at every place with a written target type, innermost first.
pub(crate) trait Convert {
    /// Whether the fields of a struct literal are such places. Off unless a pass documents them,
    /// since `#[widen]` leaves them explicit.
    const STRUCT_FIELDS: bool = false;

    /// Whether the right side of a compound assignment (`+=`, shifts excepted) is such a place.
    const COMPOUND_ASSIGNMENTS: bool = true;

    /// Rewrites `value`, which goes where `target` is written; `source` is the type of `value`
    /// when the input writes it.
    fn convert(&mut self, target: &Type, value: &mut Expr, source: Option<&Type>);

    /// Whether the fn bodies inside the item with `attrs` (first the inner attributes of the
    /// whole input) are walked, taking the pass's marker off them; a pass without a marker walks
    /// every body.
    fn marks(&mut self, _attrs: &mut Vec<Attribute>) -> bool {
        true
    }

    /// Sees each attribute `marks` left in the input, so a marker out of place can be reported.
    fn attribute(&mut self, _attr: &Attribute) {}
}

/// Calls `convert` at every typed place in the fn bodies of `file` it marks: fns, methods and
/// trait default methods, each walked on its own.
pub(crate) fn walk_file(file: &mut syn::File, convert: &mut impl Convert) {
    let declared = Declared::collect(file);
    let on = convert.marks(&mut file.attrs);
    Bodies {
        declared: &declared,
        on,
        owner: None,
        convert,
    }
    .visit_file_mut(file);
}

/// The walk over the items of the input that finds the fn bodies to convert.
struct Bodies<'a, C> {
    declared: &'a Declared,
    on: bool,
    /// The type of the enclosing `impl`, for `Self` and `self`.
    owner: Option<String>,
    convert: &'a mut C,
}

impl<C: Convert> Bodies<'_, C> {
    fn within(&mut self, on: bool, owner: Option<String>, visit: impl FnOnce(&mut Self)) {
        let outer_on = self.on;
        self.on |= on;
        let outer_owner = mem::replace(&mut self.owner, owner);
        visit(self);
        self.on = outer_on;
        self.owner = outer_owner;
    }

    fn body(&mut self, sig: &mut Signature, block: &mut Block) {
        if self.on {
            walk_fn(
                self.declared,
                self.owner.as_deref(),
                sig,
                block,
                self.convert,
            );
        }
    }
}

impl<C: Convert> VisitMut for Bodies<'_, C> {
    fn visit_item_mod_mut(&mut self, item: &mut ItemMod) {
        let on = self.convert.marks(&mut item.attrs);
        self.within(on, None, |bodies| {
            visit_mut::visit_item_mod_mut(bodies, item);
        });
    }

    fn visit_item_impl_mut(&mut self, item: &mut ItemImpl) {
        let on = self.convert.marks(&mut item.attrs);
        self.within(on, Some(type_name(&item.self_ty)), |bodies| {
            visit_mut::visit_item_impl_mut(bodies, item);
        });
    }

    fn visit_item_fn_mut(&mut self, item: &mut ItemFn) {
        let on = self.convert.marks(&mut item.attrs);
        self.within(on, None, |bodies| {
            bodies.body(&mut item.sig, &mut item.block);
            visit_mut::visit_item_fn_mut(bodies, item);
        });
    }

    fn visit_impl_item_fn_mut(&mut self, item: &mut ImplItemFn) {
        let on = self.convert.marks(&mut item.attrs);
        self.within(on, self.owner.clone(), |bodies| {
            bodies.body(&mut item.sig, &mut item.block);
            visit_mut::visit_impl_item_fn_mut(bodies, item);
        });
    }

    fn visit_trait_item_fn_mut(&mut self, item: &mut TraitItemFn) {
        let on = self.convert.marks(&mut item.attrs);
        self.within(on, None, |bodies| {
            if let Some(block) = &mut item.default {
                bodies.body(&mut item.sig, block);
            }
            visit_mut::visit_trait_item_fn_mut(bodies, item);
        });
    }

    fn visit_attribute_mut(&mut self, attr: &mut Attribute) {
        self.convert.attribute(attr);
    }
}

/// The structs and fns declared anywhere in the input.
#[derive(Default)]
struct Declared {
    /// Field types by struct name, tuple fields by index; `None` for a name declared twice with
    /// different fields, which the name alone cannot tell apart.
    structs: HashMap<String, Option<Vec<(String, Type)>>>,
    fns: Fns,
}

impl Declared {
    fn collect(file: &mut syn::File) -> Self {
        let mut collect = Collect {
            declared: Self::default(),
            owner: None,
        };
        collect.visit_file_mut(file);
        collect.declared
    }

    fn field(&self, owner: &str, member: &Member) -> Option<Type> {
        let key = match member {
            Member::Named(name) => name.to_string(),
            Member::Unnamed(index) => index.index.to_string(),
        };
        let fields = self.structs.get(owner)?.as_ref()?;
        fields
            .iter()
            .find(|(name, _)| *name == key)
            .map(|(_, ty)| ty.clone())
    }
}

struct Collect {
    declared: Declared,
    owner: Option<String>,
}

impl VisitMut for Collect {
    fn visit_item_struct_mut(&mut self, item: &mut ItemStruct) {
        let fields: Vec<(String, Type)> = item
            .fields
            .iter()
            .enumerate()
            .map(|(at, field)| {
                let name = field
                    .ident
                    .as_ref()
                    .map_or_else(|| at.to_string(), ToString::to_string);
                (name, field.ty.clone())
            })
            .collect();
        let known = (self.declared.structs)
            .entry(item.ident.to_string())
            .or_insert_with(|| Some(fields.clone()));
        if known.as_ref() != Some(&fields) {
            *known = None;
        }
    }

    fn visit_item_impl_mut(&mut self, item: &mut ItemImpl) {
        let outer = self.owner.replace(type_name(&item.self_ty));
        visit_mut::visit_item_impl_mut(self, item);
        self.owner = outer;
    }

    fn visit_item_fn_mut(&mut self, item: &mut ItemFn) {
        self.declared.fns.add(&item.sig, None);
        let outer = self.owner.take();
        visit_mut::visit_item_fn_mut(self, item);
        self.owner = outer;
    }

    fn visit_impl_item_fn_mut(&mut self, item: &mut ImplItemFn) {
        self.declared
            .fns
            .add(&item.sig, Some(self.owner.clone().unwrap_or_default()));
        visit_mut::visit_impl_item_fn_mut(self, item);
    }
}

/// Calls `convert` at every typed place in the body of a fn; `owner` is the type of the
/// enclosing `impl`, which `Self` and `self` stand for. Items nested in the body are left alone:
/// they do not see its locals, so each is walked on its own.
fn walk_fn(
    declared: &Declared,
    owner: Option<&str>,
    sig: &mut Signature,
    block: &mut Block,
    convert: &mut impl Convert,
) {
    let mut walk = Walk {
        declared,
        owner,
        frames: vec![HashMap::new()],
        convert,
    };
    for arg in &mut sig.inputs {
        match arg {
            FnArg::Receiver(receiver) => {
                let ty = match &receiver.kind {
                    ReceiverKind::Typed(_, ty) => (**ty).clone(),
                    _ => syn::parse_quote!(Self),
                };
                walk.declare("self".to_owned(), Some(ty));
            }
            FnArg::Typed(param) => walk.bind(&mut param.pat, Some(&param.ty)),
        }
    }
    walk.visit_block_mut(block);
}

struct Walk<'a, C> {
    declared: &'a Declared,
    owner: Option<&'a str>,
    /// The names in scope, innermost last; `None` for a name whose type is not written.
    frames: Vec<HashMap<String, Option<Type>>>,
    convert: &'a mut C,
}

impl<C: Convert> Walk<'_, C> {
    fn scoped(&mut self, visit: impl FnOnce(&mut Self)) {
        self.frames.push(HashMap::new());
        visit(self);
        self.frames.pop();
    }

    fn declare(&mut self, name: String, ty: Option<Type>) {
        if let Some(frame) = self.frames.last_mut() {
            frame.insert(name, ty);
        }
    }

    /// Binds the names of `pat`: a plain name gets the type written on it or `ty`, any other
    /// name becomes unknown.
    fn bind(&mut self, pat: &mut Pat, ty: Option<&Type>) {
        let (pat, ty) = match pat {
            Pat::Type(typed) => (&mut *typed.pat, Some(&*typed.ty)),
            pat => (pat, ty),
        };
        if let (Pat::Ident(plain), Some(ty)) = (&*pat, ty)
            && plain.by_ref.is_none()
            && plain.subpat.is_none()
        {
            self.declare(plain.ident.to_string(), Some(ty.clone()));
            return;
        }
        let mut names = Names::default();
        names.visit_pat_mut(pat);
        for name in names.0 {
            self.declare(name, None);
        }
    }

    fn type_of(&self, expr: &Expr) -> Option<Type> {
        match expr {
            Expr::Path(path) if path.qself.is_none() => {
                let name = path.path.get_ident()?.to_string();
                self.frames
                    .iter()
                    .rev()
                    .find_map(|frame| frame.get(&name))
                    .cloned()
                    .flatten()
            }
            Expr::Paren(inner) => self.type_of(&inner.expr),
            Expr::Unary(unary) if matches!(unary.op, UnOp::Deref(_)) => {
                match self.type_of(&unary.expr)? {
                    Type::Reference(reference) => Some(*reference.elem),
                    _ => None,
                }
            }
            Expr::Field(field) => {
                let base = self.type_of(&field.base)?;
                self.declared
                    .field(&self.struct_name(&base)?, &field.member)
            }
            _ => None,
        }
    }

    /// The declared struct a value of type `ty` is, through references, by its last segment.
    fn struct_name(&self, ty: &Type) -> Option<String> {
        match ty {
            Type::Reference(reference) => self.struct_name(&reference.elem),
            Type::Path(path) if path.qself.is_none() => self.named(&path.path),
            _ => None,
        }
    }

    /// The struct a path names, by its last segment, with `Self` standing for the owner.
    fn named(&self, path: &Path) -> Option<String> {
        let name = path.segments.last()?.ident.to_string();
        if name == "Self" {
            self.owner.map(str::to_owned)
        } else {
            Some(name)
        }
    }

    fn convert_to(&mut self, target: &Type, value: &mut Expr) {
        let source = self.type_of(value);
        self.convert.convert(target, value, source.as_ref());
    }

    fn assign_to(&mut self, place: &Expr, value: &mut Expr) {
        if let Some(target) = self.type_of(place) {
            self.convert_to(&target, value);
        }
    }

    /// Converts each argument whose parameter has the same written type in every fn the call
    /// can resolve to.
    fn call_arguments(&mut self, call: &mut ExprCall) {
        let Some((name, qualifier)) = path_target(&call.func, self.owner) else {
            return;
        };
        let shapes = self
            .declared
            .fns
            .path_call(&name.to_string(), qualifier.as_deref());
        let Some((first, rest)) = shapes.split_first() else {
            return;
        };
        if shapes
            .iter()
            .any(|shape| shape.params.len() != call.args.len())
        {
            return;
        }
        for (at, arg) in call.args.iter_mut().enumerate() {
            let target = &first.params[at].ty;
            if rest.iter().all(|shape| shape.params[at].ty == *target) {
                self.convert_to(target, arg);
            }
        }
    }

    /// Converts each field of a literal of a declared struct. A shorthand field that changes
    /// gets its `name:` back, since the shorthand can only name the binding.
    fn struct_fields(&mut self, literal: &mut ExprStruct) {
        if literal.qself.is_some() {
            return;
        }
        let Some(owner) = self.named(&literal.path) else {
            return;
        };
        for field in &mut literal.fields {
            let Some(target) = self.declared.field(&owner, &field.member) else {
                continue;
            };
            let shorthand = field.colon_token.is_none().then(|| field.expr.clone());
            self.convert_to(&target, &mut field.expr);
            if shorthand.is_some_and(|before| before != field.expr) {
                field.colon_token = Some(Token![:](field.expr.span()));
            }
        }
    }
}

impl<C: Convert> VisitMut for Walk<'_, C> {
    fn visit_block_mut(&mut self, block: &mut Block) {
        self.scoped(|walk| {
            for stmt in &mut block.stmts {
                walk.visit_stmt_mut(stmt);
            }
        });
    }

    fn visit_item_mut(&mut self, _item: &mut Item) {}

    fn visit_local_mut(&mut self, local: &mut Local) {
        if let Some(init) = &mut local.init {
            self.visit_expr_mut(&mut init.expr);
            if let Some((_, diverge)) = &mut init.diverge {
                self.visit_expr_mut(diverge);
            }
            if let Pat::Type(typed) = &local.pat {
                self.convert_to(&typed.ty, &mut init.expr);
            }
        }
        self.bind(&mut local.pat, None);
    }

    fn visit_expr_closure_mut(&mut self, closure: &mut ExprClosure) {
        self.scoped(|walk| {
            for input in &mut closure.inputs {
                walk.bind(input, None);
            }
            walk.visit_expr_mut(&mut closure.body);
        });
    }

    fn visit_expr_for_loop_mut(&mut self, expr: &mut ExprForLoop) {
        self.visit_expr_mut(&mut expr.expr);
        let element = self.type_of(&expr.expr).and_then(|ty| element(&ty));
        self.scoped(|walk| {
            walk.bind(&mut expr.pat, element.as_ref());
            walk.visit_block_mut(&mut expr.body);
        });
    }

    fn visit_expr_if_mut(&mut self, expr: &mut ExprIf) {
        self.scoped(|walk| {
            walk.visit_expr_mut(&mut expr.cond);
            walk.visit_block_mut(&mut expr.then_branch);
        });
        if let Some((_, else_branch)) = &mut expr.else_branch {
            self.visit_expr_mut(else_branch);
        }
    }

    fn visit_expr_while_mut(&mut self, expr: &mut ExprWhile) {
        self.scoped(|walk| {
            walk.visit_expr_mut(&mut expr.cond);
            walk.visit_block_mut(&mut expr.body);
        });
    }

    fn visit_expr_let_mut(&mut self, expr: &mut ExprLet) {
        self.visit_expr_mut(&mut expr.expr);
        self.bind(&mut expr.pat, None);
    }

    fn visit_arm_mut(&mut self, arm: &mut Arm) {
        self.scoped(|walk| {
            walk.bind(&mut arm.pat, None);
            walk.visit_pat_mut(&mut arm.pat);
            walk.visit_expr_mut(&mut arm.body);
        });
    }

    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        visit_mut::visit_expr_mut(self, expr);
        match expr {
            Expr::Assign(assign) => self.assign_to(&assign.left, &mut assign.right),
            // Shifts take any integer on the right, so a conversion there would change valid code.
            Expr::Binary(binary)
                if C::COMPOUND_ASSIGNMENTS
                    && matches!(
                        binary.op,
                        BinOp::AddAssign(_)
                            | BinOp::SubAssign(_)
                            | BinOp::MulAssign(_)
                            | BinOp::DivAssign(_)
                            | BinOp::RemAssign(_)
                            | BinOp::BitAndAssign(_)
                            | BinOp::BitOrAssign(_)
                            | BinOp::BitXorAssign(_)
                    ) =>
            {
                self.assign_to(&binary.left, &mut binary.right);
            }
            Expr::Call(call) => self.call_arguments(call),
            Expr::Struct(literal) if C::STRUCT_FIELDS => self.struct_fields(literal),
            _ => {}
        }
    }
}

/// The type of one element when iterating over a value of type `ty`: a slice, array or `Vec`,
/// by value or through a reference.
fn element(ty: &Type) -> Option<Type> {
    match ty {
        Type::Reference(reference) => {
            let elem = match &*reference.elem {
                Type::Slice(slice) => (*slice.elem).clone(),
                owned => owned_element(owned)?,
            };
            Some(Type::Reference(TypeReference {
                elem: Box::new(elem),
                ..reference.clone()
            }))
        }
        owned => owned_element(owned),
    }
}

fn owned_element(ty: &Type) -> Option<Type> {
    if let Type::Array(array) = ty {
        return Some((*array.elem).clone());
    }
    std_argument(ty, "vec", "Vec").cloned()
}

/// Every name a pattern binds, and any name that only looks like one (a unit struct, a
/// constant): treating those as unknown too costs nothing.
#[derive(Default)]
struct Names(Vec<String>);

impl VisitMut for Names {
    fn visit_pat_ident_mut(&mut self, pat: &mut PatIdent) {
        self.0.push(pat.ident.to_string());
        visit_mut::visit_pat_ident_mut(self, pat);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::ToTokens;

    #[test]
    fn iterating_a_slice_array_or_vec_yields_its_element_through_the_same_reference() {
        let element_of = |source: &str| {
            element(&syn::parse_str(source).unwrap()).map(|ty| ty.to_token_stream().to_string())
        };
        for (iterated, yielded) in [
            ("&[u8]", "& u8"),
            ("&mut [u8]", "& mut u8"),
            ("[u8; 4]", "u8"),
            ("&[u8; 4]", "& u8"),
            ("Vec<u8>", "u8"),
            ("&std::vec::Vec<u8>", "& u8"),
        ] {
            assert_eq!(element_of(iterated).as_deref(), Some(yielded), "{iterated}");
        }
        for other in ["u8", "Option<u8>", "&str", "my::Vec<u8>", "[u8]"] {
            assert_eq!(element_of(other), None, "{other}");
        }
    }

    /// Wraps every value going into a written `u64` in parentheses.
    struct Wrap;

    impl Convert for Wrap {
        const STRUCT_FIELDS: bool = true;

        fn convert(&mut self, target: &Type, value: &mut Expr, _source: Option<&Type>) {
            if target.to_token_stream().to_string() == "u64" {
                *value = syn::parse_quote!((#value));
            }
        }
    }

    #[test]
    fn struct_literal_fields_are_typed_places_and_a_changed_shorthand_gets_its_name_back() {
        let mut file: syn::File = syn::parse_quote! {
            struct S { a: u64, b: u8 }
            impl S { fn new(a: u64, b: u8) -> Self { Self { a, b } } }
            fn f(a: u64, b: u8) { S { a, b: b }; S { a: 1, b }; T { a }; }
        };
        walk_file(&mut file, &mut Wrap);
        let expected: syn::File = syn::parse_quote! {
            struct S { a: u64, b: u8 }
            impl S { fn new(a: u64, b: u8) -> Self { Self { a: (a), b } } }
            fn f(a: u64, b: u8) { S { a: (a), b: b }; S { a: (1), b }; T { a }; }
        };
        assert_eq!(
            file.to_token_stream().to_string(),
            expected.to_token_stream().to_string()
        );
    }

    #[test]
    fn a_struct_declared_twice_with_other_fields_has_no_known_fields() {
        let mut file: syn::File = syn::parse_quote! {
            struct A { v: u8 }
            mod m { pub struct A { v: u16 } pub struct B(u32); pub struct C { v: u8 } }
            struct C { v: u8 }
        };
        let declared = Declared::collect(&mut file);
        let field = |owner: &str, member: Member| {
            declared
                .field(owner, &member)
                .map(|ty| ty.to_token_stream().to_string())
        };
        assert_eq!(field("A", syn::parse_quote!(v)), None);
        assert_eq!(field("B", syn::parse_quote!(0)).as_deref(), Some("u32"));
        assert_eq!(field("C", syn::parse_quote!(v)).as_deref(), Some("u8"));
    }
}
