use clippy_utils::diagnostics::span_lint_hir_and_then;
use rustc_data_structures::fx::FxHashMap;
use rustc_errors::Applicability;
use rustc_hir::def::{DefKind, Res};
use rustc_hir::def_id::LocalDefId;
use rustc_hir::{
    self as hir, BorrowKind, ExprKind, ImplItemKind, ItemKind, Mutability, PatKind, QPath, StmtKind, StructTailExpr,
    TyKind, VariantData,
};
use rustc_lint::LateContext;
use rustc_middle::ty::{self, Ty};
use rustc_span::{MultiSpan, Symbol, kw};

use crate::convert::manual_display::under_sugared;
use crate::hir_util::{self, is_hand_written, local, peel_blocks};

dylint_linting::declare_lint! {
    /// ### What it does
    ///
    /// Flags a struct with named fields whose inherent impl holds exactly what `#[rustx::data]`
    /// generates: `new` taking every field in order, a getter per field, and optionally a
    /// `set_*` or `with_*` method per field.
    ///
    /// ### Why is this bad?
    ///
    /// Each field is spelled four or five times (field, parameter, initialiser, getter, setter),
    /// and adding a field means editing all of them. `#[rustx::data]` generates the same methods
    /// from the struct.
    ///
    /// Getters must follow the rule `#[data]` uses, judged by the field's written type: a
    /// primitive (`u8`, `bool`, ...) or a shared reference is returned by value, a `String` as
    /// `&str`, anything else as `&T`. The struct must not be generic. The fix is
    /// machine-applicable when every matched method is `pub` and has no attributes or docs, since
    /// the generated ones are `pub` and undocumented.
    ///
    /// ### Example
    ///
    /// ```rust,ignore
    /// pub struct User {
    ///     id: u64,
    ///     name: String,
    /// }
    ///
    /// impl User {
    ///     pub fn new(id: u64, name: String) -> Self {
    ///         Self { id, name }
    ///     }
    ///     pub fn id(&self) -> u64 {
    ///         self.id
    ///     }
    ///     pub fn name(&self) -> &str {
    ///         &self.name
    ///     }
    /// }
    /// ```
    ///
    /// Use instead:
    ///
    /// ```rust,ignore
    /// #[rustx::data]
    /// pub struct User {
    ///     id: u64,
    ///     name: String,
    /// }
    /// ```
    pub RUSTX_MANUAL_DATA,
    Warn,
    "a hand-written constructor and getters that `#[rustx::data]` generates"
}

/// The written field types `#[data]` returns by value (`data/methods.rs`, `COPY`).
const COPY: &[&str] = &[
    "bool", "char", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64", "i128", "isize", "f32",
    "f64",
];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Getter {
    Value,
    Str,
    Ref,
}

struct Field<'tcx> {
    name: Symbol,
    ty: Ty<'tcx>,
    getter: Getter,
}

pub fn check<'tcx>(cx: &LateContext<'tcx>, item: &hir::Item<'tcx>) {
    let ItemKind::Struct(_, generics, VariantData::Struct { fields, .. }) = item.kind else {
        return;
    };
    let adt = item.owner_id.def_id;
    if !generics.params.is_empty() || fields.is_empty() || !is_hand_written(cx, adt, item.span, "struct") {
        return;
    }
    let tcx = cx.tcx;
    let fields: Vec<Field<'tcx>> = fields
        .iter()
        .map(|field| Field {
            name: field.ident.name,
            ty: tcx.type_of(field.def_id).instantiate_identity().skip_norm_wip(),
            getter: getter_kind(field.ty),
        })
        .collect();
    let inherent = tcx.inherent_impls(adt);
    // `#[data]` is already on the struct: its methods would collide with these anyway.
    if inherent
        .iter()
        .any(|id| hir_util::from_rustx(cx, tcx.expn_that_defined(*id)))
    {
        return;
    }
    for impl_id in inherent.iter().filter_map(|id| id.as_local()) {
        if let Some(found) = matched(cx, impl_id, adt, &fields) {
            emit(cx, item, impl_id, &found);
            return;
        }
    }
}

struct Found {
    /// The matched methods.
    methods: Vec<LocalDefId>,
    setters: bool,
    with: bool,
    /// Whether the impl holds nothing but the matched methods.
    whole_impl: bool,
}

fn matched(cx: &LateContext<'_>, impl_id: LocalDefId, adt: LocalDefId, fields: &[Field<'_>]) -> Option<Found> {
    let tcx = cx.tcx;
    let item = tcx.hir_expect_item(impl_id);
    let ItemKind::Impl(imp) = item.kind else {
        return None;
    };
    if !imp.generics.params.is_empty() || !is_hand_written(cx, impl_id, item.span, "impl") {
        return None;
    }
    let fns: FxHashMap<Symbol, &hir::ImplItem<'_>> = imp
        .items
        .iter()
        .map(|id| tcx.hir_impl_item(*id))
        .filter(|item| matches!(item.kind, ImplItemKind::Fn(..)) && item.generics.params.is_empty())
        .map(|item| (item.ident.name, item))
        .collect();
    let self_ty = tcx.type_of(adt).instantiate_identity().skip_norm_wip();
    let method = |name: &str| fns.get(&Symbol::intern(name)).copied();

    let new = method("new")?;
    is_new(cx, new, self_ty, adt, fields)?;
    let mut methods = vec![new.owner_id.def_id];
    for field in fields {
        let getter = method(field.name.as_str())?;
        is_getter(cx, getter, self_ty, field)?;
        methods.push(getter.owner_id.def_id);
    }
    let mut set = |prefix: &str, check: fn(&LateContext<'_>, &hir::ImplItem<'_>, Ty<'_>, &Field<'_>) -> Option<()>| {
        let found: Option<Vec<LocalDefId>> = fields
            .iter()
            .map(|field| {
                let method = method(&format!("{prefix}{}", field.name))?;
                check(cx, method, self_ty, field)?;
                Some(method.owner_id.def_id)
            })
            .collect();
        found.map(|found| methods.extend(found)).is_some()
    };
    let setters = set("set_", is_setter);
    let with = set("with_", is_with);
    let whole_impl = methods.len() == imp.items.len();
    Some(Found { methods, setters, with, whole_impl })
}

fn emit(cx: &LateContext<'_>, item: &hir::Item<'_>, impl_id: LocalDefId, found: &Found) {
    let tcx = cx.tcx;
    let flags: Vec<&str> = [(found.setters, "setters"), (found.with, "with")]
        .into_iter()
        .filter_map(|(on, flag)| on.then_some(flag))
        .collect();
    let path = if under_sugared(cx, item.owner_id.def_id) { "data" } else { "rustx::data" };
    let attr = if flags.is_empty() {
        format!("#[{path}]")
    } else {
        format!("#[{path}({})]", flags.join(", "))
    };
    let mut exact = true;
    let mut edits = vec![hir_util::insert_attr(cx, item.span, &attr)];
    let mut spans = MultiSpan::new();
    for &method in &found.methods {
        let hir_id = tcx.local_def_id_to_hir_id(method);
        let span = tcx.hir_impl_item(hir::ImplItemId { owner_id: hir::OwnerId { def_id: method } }).span;
        spans.push_primary_span(span);
        exact &= tcx.visibility(method).is_public() && tcx.hir_attrs(hir_id).is_empty();
        if !found.whole_impl {
            edits.push(hir_util::remove_with_attrs(cx, hir_id, span));
        }
    }
    if found.whole_impl {
        let impl_hir = tcx.local_def_id_to_hir_id(impl_id);
        exact &= tcx.hir_attrs(impl_hir).is_empty();
        edits.push(hir_util::remove_with_attrs(cx, impl_hir, tcx.hir_expect_item(impl_id).span));
    }
    spans.push_span_label(item.span.shrink_to_lo(), "for this struct");
    let applicability = if exact {
        hir_util::applicability(cx, true)
    } else {
        Applicability::MaybeIncorrect
    };
    span_lint_hir_and_then(
        cx,
        RUSTX_MANUAL_DATA,
        tcx.local_def_id_to_hir_id(found.methods[0]),
        spans,
        format!("this constructor and these accessors can be converted to `{attr}`"),
        |diag| {
            if !exact {
                diag.note(
                    "`#[data]` generates `pub` methods without docs or attributes; these differ, so \
                     check the rewrite",
                );
            }
            hir_util::note_dependency(cx, diag);
            diag.multipart_suggestion(format!("generate them with `{attr}`"), edits, applicability);
        },
    );
}

/// How `#[data]` returns a field of this written type.
fn getter_kind(ty: &hir::Ty<'_>) -> Getter {
    match ty.kind {
        TyKind::Ref(_, mt) if mt.mutbl == Mutability::Not => Getter::Value,
        TyKind::Path(QPath::Resolved(None, path)) if path.segments.iter().all(|s| s.args.is_none()) => {
            let names: Vec<&str> = path
                .segments
                .iter()
                .map(|s| s.ident.name)
                .filter(|name| *name != kw::PathRoot)
                .map(|name| name.as_str())
                .collect();
            match names.as_slice() {
                [one] if COPY.contains(one) => Getter::Value,
                ["String"] | ["std" | "alloc", "string", "String"] => Getter::Str,
                _ => Getter::Ref,
            }
        }
        _ => Getter::Ref,
    }
}

fn signature<'tcx>(cx: &LateContext<'tcx>, method: &hir::ImplItem<'_>) -> ty::FnSig<'tcx> {
    cx.tcx
        .fn_sig(method.owner_id)
        .instantiate_identity()
        .skip_norm_wip()
        .skip_binder()
}

fn body<'tcx>(cx: &LateContext<'tcx>, method: &hir::ImplItem<'_>) -> Option<&'tcx hir::Body<'tcx>> {
    let ImplItemKind::Fn(_, body_id) = method.kind else {
        return None;
    };
    Some(cx.tcx.hir_body(body_id))
}

fn binding(param: &hir::Param<'_>) -> Option<(hir::HirId, Symbol)> {
    match param.pat.kind {
        PatKind::Binding(_, id, ident, None) => Some((id, ident.name)),
        _ => None,
    }
}

fn is_new(cx: &LateContext<'_>, new: &hir::ImplItem<'_>, self_ty: Ty<'_>, adt: LocalDefId, fields: &[Field<'_>]) -> Option<()> {
    let sig = signature(cx, new);
    let body = body(cx, new)?;
    if body.params.len() != fields.len() || sig.output() != self_ty {
        return None;
    }
    let mut params = FxHashMap::default();
    for ((param, field), ty) in body.params.iter().zip(fields).zip(sig.inputs()) {
        let (id, name) = binding(param)?;
        if name != field.name || *ty != field.ty {
            return None;
        }
        params.insert(name, id);
    }
    let value = peel_blocks(body.value);
    let ExprKind::Struct(qpath, inits, StructTailExpr::None) = value.kind else {
        return None;
    };
    let target = match cx.qpath_res(qpath, value.hir_id) {
        Res::SelfTyAlias { .. } => true,
        Res::Def(DefKind::Struct, id) => id == adt.to_def_id(),
        _ => false,
    };
    (target
        && inits.len() == fields.len()
        && inits
            .iter()
            .all(|init| params.get(&init.ident.name).is_some_and(|id| local(init.expr) == Some(*id))))
    .then_some(())
}

/// `self.field`, with `self` the method's first parameter.
fn is_self_field(expr: &hir::Expr<'_>, this: hir::HirId, field: Symbol) -> bool {
    matches!(expr.kind, ExprKind::Field(base, name) if name.name == field && local(base) == Some(this))
}

fn is_getter(cx: &LateContext<'_>, getter: &hir::ImplItem<'_>, self_ty: Ty<'_>, field: &Field<'_>) -> Option<()> {
    let sig = signature(cx, getter);
    let body = body(cx, getter)?;
    let [this] = body.params else {
        return None;
    };
    let (this, _) = binding(this)?;
    let [receiver] = sig.inputs() else {
        return None;
    };
    if !matches!(receiver.kind(), ty::Ref(_, inner, Mutability::Not) if *inner == self_ty) {
        return None;
    }
    let output = sig.output();
    let value = peel_blocks(body.value);
    let ok = match field.getter {
        Getter::Value => output == field.ty && is_self_field(value, this, field.name),
        Getter::Str | Getter::Ref => {
            let returns = match output.kind() {
                ty::Ref(_, inner, Mutability::Not) => {
                    if field.getter == Getter::Str {
                        inner.is_str()
                    } else {
                        *inner == field.ty
                    }
                }
                _ => false,
            };
            returns
                && matches!(value.kind, ExprKind::AddrOf(BorrowKind::Ref, Mutability::Not, inner)
                    if is_self_field(inner, this, field.name))
        }
    };
    ok.then_some(())
}

/// The one statement `self.field = value;` of a setter or `with_` body.
fn assigns(body: &hir::Body<'_>, this: hir::HirId, value: hir::HirId, field: Symbol) -> Option<Option<&hir::Expr<'_>>> {
    let ExprKind::Block(block, None) = body.value.kind else {
        return None;
    };
    let [stmt] = block.stmts else {
        return None;
    };
    let (StmtKind::Semi(assign) | StmtKind::Expr(assign)) = stmt.kind else {
        return None;
    };
    let ExprKind::Assign(lhs, rhs, _) = assign.kind else {
        return None;
    };
    (is_self_field(lhs, this, field) && local(rhs) == Some(value)).then_some(block.expr)
}

fn is_setter(cx: &LateContext<'_>, setter: &hir::ImplItem<'_>, self_ty: Ty<'_>, field: &Field<'_>) -> Option<()> {
    let sig = signature(cx, setter);
    let body = body(cx, setter)?;
    let [this, value] = body.params else {
        return None;
    };
    let ([receiver, ty], (this, _), (value, _)) = (sig.inputs(), binding(this)?, binding(value)?) else {
        return None;
    };
    let receiver_ok = matches!(receiver.kind(), ty::Ref(_, inner, Mutability::Mut) if *inner == self_ty);
    (receiver_ok && *ty == field.ty && sig.output().is_unit() && assigns(body, this, value, field.name)? .is_none())
        .then_some(())
}

fn is_with(cx: &LateContext<'_>, with: &hir::ImplItem<'_>, self_ty: Ty<'_>, field: &Field<'_>) -> Option<()> {
    let sig = signature(cx, with);
    let body = body(cx, with)?;
    let [this, value] = body.params else {
        return None;
    };
    let ([receiver, ty], (this, _), (value, _)) = (sig.inputs(), binding(this)?, binding(value)?) else {
        return None;
    };
    let tail = assigns(body, this, value, field.name)??;
    (*receiver == self_ty && *ty == field.ty && sig.output() == self_ty && local(tail) == Some(this)).then_some(())
}
