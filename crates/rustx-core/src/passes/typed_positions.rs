//! Places where a value goes into a type the input writes, for the conversion passes (problems
//! 13 and 15), which have no type information of their own.
//!
//! A target type counts only when it is written: a `let` annotation, a local or a field of a struct
//! declared in the input (assigned or compound-assigned), and a parameter of a fn declared in the
//! input, called by path (calls resolve by name, as call-site defaults do). The type of a value is known only for a name bound
//! with a written type (a `let` annotation, a parameter, a closure parameter, the element of a
//! `for` loop over a slice, array or `Vec` of known type), a field of a declared struct reached
//! from one, and `*` of a known reference. Any other binding of a name ends what is known about
//! it, so a shadowing `let x = ..` never leaves a stale type behind.
//!
//! Struct and fn names are matched by their last path segment across the whole input, so the
//! types a conversion pass is given are guesses it must make safe: it should spell the conversion
//! so that a wrong guess fails to compile.

use std::collections::HashMap;

use syn::visit_mut::{self, VisitMut};
use syn::{
    Arm, BinOp, Block, Expr, ExprCall, ExprClosure, ExprForLoop, ExprIf, ExprLet, ExprWhile, FnArg,
    GenericArgument, ImplItemFn, Item, ItemFn, ItemImpl, ItemStruct, Local, Member, Pat, PatIdent,
    PathArguments, ReceiverKind, Signature, Type, TypeReference, UnOp,
};

use super::call_defaults::collect::{Fns, path_target, type_name};
use crate::tokens::syntax::std_type;

/// A conversion pass: called at every place with a written target type, innermost first.
pub(super) trait Convert {
    /// Rewrites `value`, which goes where `target` is written; `source` is the type of `value`
    /// when the input writes it.
    fn convert(&mut self, target: &Type, value: &mut Expr, source: Option<&Type>);
}

/// The structs and fns declared anywhere in the input.
#[derive(Default)]
pub(super) struct Declared {
    /// Field types by struct name, tuple fields by index; `None` for a name declared twice with
    /// different fields, which the name alone cannot tell apart.
    structs: HashMap<String, Option<Vec<(String, Type)>>>,
    fns: Fns,
}

impl Declared {
    pub(super) fn collect(file: &mut syn::File) -> Self {
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
pub(super) fn walk_fn(
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
            Type::Path(path) if path.qself.is_none() => {
                let name = path.path.segments.last()?.ident.to_string();
                if name == "Self" {
                    self.owner.map(str::to_owned)
                } else {
                    Some(name)
                }
            }
            _ => None,
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
                if matches!(
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
    match std_type(ty, "vec", "Vec")? {
        PathArguments::AngleBracketed(args) if args.args.len() == 1 => match args.args.first()? {
            GenericArgument::Type(elem) => Some(elem.clone()),
            _ => None,
        },
        _ => None,
    }
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
