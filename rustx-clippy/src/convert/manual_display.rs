use clippy_utils::diagnostics::span_lint_hir_and_then;
use clippy_utils::macros::root_macro_call_first_node;
use clippy_utils::source::snippet_opt;
use clippy_utils::ty::implements_trait;
use rustc_data_structures::fx::FxHashMap;
use rustc_errors::Applicability;
use rustc_hir::def::{CtorOf, DefKind, Res};
use rustc_hir::def_id::LocalDefId;
use rustc_hir::{self as hir, ExprKind, ImplItemKind, ItemKind, MatchSource, PatExprKind, PatKind, UnOp};
use rustc_lint::LateContext;
use rustc_ast::LitKind;
use rustc_middle::ty::{AdtDef, VariantDef};
use rustc_span::{MultiSpan, Span, sym};

use crate::convert::write_call::{self, FieldRef};
use crate::hir_util::{self, is_hand_written, local, peel_blocks};

dylint_linting::declare_lint! {
    /// ### What it does
    ///
    /// Flags a hand-written `impl Display` for an enum in which every variant is printed by one
    /// `write!(f, "..", fields)` of its own fields, the impl `#[rustx::display]` generates from
    /// `#[display("..")]` on the variants.
    ///
    /// ### Why is this bad?
    ///
    /// The message of each variant sits in a `match` far from the variant, and the impl repeats
    /// every variant's name and fields. `#[display("..")]` keeps the message on the variant,
    /// and the placeholders name its fields (`{0}`, `{key}`), which the macro checks.
    ///
    /// `#[rustx::display]` also implements `std::error::Error` (without `source`). The fix is
    /// machine-applicable when the enum already has an empty `impl Error`, which it removes;
    /// otherwise the enum gains an `Error` impl and the fix is only suggested.
    ///
    /// ### Example
    ///
    /// ```rust,ignore
    /// #[derive(Debug)]
    /// pub enum LoadError {
    ///     Io(std::io::Error),
    ///     Missing { key: String },
    /// }
    ///
    /// impl std::fmt::Display for LoadError {
    ///     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    ///         match self {
    ///             Self::Io(e) => write!(f, "io error: {e}"),
    ///             Self::Missing { key } => write!(f, "missing key {}", key),
    ///         }
    ///     }
    /// }
    ///
    /// impl std::error::Error for LoadError {}
    /// ```
    ///
    /// Use instead:
    ///
    /// ```rust,ignore
    /// #[rustx::display]
    /// #[derive(Debug)]
    /// pub enum LoadError {
    ///     #[display("io error: {0}")]
    ///     Io(std::io::Error),
    ///     #[display("missing key {key}")]
    ///     Missing { key: String },
    /// }
    /// ```
    pub RUSTX_MANUAL_DISPLAY,
    Warn,
    "a hand-written `Display` for an enum that `#[rustx::display]` generates"
}

/// How the enum's `Display` converts to `#[display("..")]`.
pub struct DisplayPlan {
    /// The hand-written `impl Display`.
    pub impl_id: LocalDefId,
    pub impl_span: Span,
    /// One `#[display("..")]` per variant, inserted at the variant.
    pub attrs: Vec<(Span, String)>,
    /// An empty hand-written `impl Error`, which `#[rustx::display]` replaces.
    pub error_impl: Option<Span>,
    /// Whether the enum already sits under `#[rustx::sugared]`, which reads the variants' attributes
    /// without `#[rustx::display]`.
    pub under_sugared: bool,
}

impl DisplayPlan {
    /// Whether the rewrite keeps the trait impls the enum has: it does when an empty `Error` impl
    /// is replaced by the generated one.
    pub fn exact(&self) -> bool {
        self.error_impl.is_some()
    }

    /// The edits other than the attribute on the enum.
    pub fn edits(&self, cx: &LateContext<'_>) -> Vec<(Span, String)> {
        let mut edits = self.attrs.clone();
        edits.push(hir_util::remove(cx, self.impl_span));
        if let Some(span) = self.error_impl {
            edits.push(hir_util::remove(cx, span));
        }
        edits
    }
}

pub fn check(cx: &LateContext<'_>, item: &hir::Item<'_>) {
    let ItemKind::Enum(..) = item.kind else {
        return;
    };
    let Some(plan) = plan(cx, item.owner_id.def_id) else {
        return;
    };
    let mut edits = plan.edits(cx);
    if !plan.under_sugared {
        edits.push(hir_util::insert_attr(cx, item.span, "#[rustx::display]"));
    }
    let applicability = hir_util::applicability(cx, plan.exact());
    let mut spans = MultiSpan::from_span(plan.impl_span);
    spans.push_span_label(item.span.shrink_to_lo(), "for this enum");
    span_lint_hir_and_then(
        cx,
        RUSTX_MANUAL_DISPLAY,
        cx.tcx.local_def_id_to_hir_id(plan.impl_id),
        spans,
        "this `Display` impl can be converted to `#[rustx::display]`",
        |diag| {
            if plan.error_impl.is_none() {
                diag.note(
                    "`#[rustx::display]` also implements `std::error::Error` for the enum, which it \
                     does not have yet",
                );
            }
            hir_util::note_dependency(cx, diag);
            diag.multipart_suggestion(
                "put the messages on the variants",
                edits,
                applicability,
            );
        },
    );
}

/// The conversion of `adt`'s hand-written `Display`, if it is one `#[display]` would generate.
pub fn plan(cx: &LateContext<'_>, adt: LocalDefId) -> Option<DisplayPlan> {
    let tcx = cx.tcx;
    let enum_item = tcx.hir_expect_item(adt);
    let ItemKind::Enum(_, generics, enum_def) = enum_item.kind else {
        return None;
    };
    if !generics.params.is_empty() || !is_hand_written(cx, adt, enum_item.span, "enum") {
        return None;
    }
    let def = tcx.adt_def(adt);
    let display = tcx.get_diagnostic_item(sym::Display)?;
    let impl_id = single_impl(cx, display, adt)?;
    let impl_item = tcx.hir_expect_item(impl_id);
    if !is_hand_written(cx, impl_id, impl_item.span, "impl") {
        return None;
    }
    let ItemKind::Impl(imp) = impl_item.kind else {
        return None;
    };
    let [fmt] = imp.items else {
        return None;
    };
    let fmt = tcx.hir_impl_item(*fmt);
    let ImplItemKind::Fn(_, body_id) = fmt.kind else {
        return None;
    };
    let body = tcx.hir_body(body_id);
    let [this, formatter] = body.params else {
        return None;
    };
    let (PatKind::Binding(_, this, ..), PatKind::Binding(_, formatter_id, formatter, None)) =
        (this.pat.kind, formatter.pat.kind)
    else {
        return None;
    };
    let ExprKind::Match(scrutinee, arms, MatchSource::Normal) = peel_blocks(body.value).kind else {
        return None;
    };
    let scrutinee = match scrutinee.kind {
        ExprKind::Unary(UnOp::Deref, inner) => inner,
        _ => scrutinee,
    };
    if local(scrutinee) != Some(this) || arms.len() != def.variants().len() {
        return None;
    }

    let mut messages: FxHashMap<usize, String> = FxHashMap::default();
    for arm in arms {
        if arm.guard.is_some() {
            return None;
        }
        let (index, bindings) = variant_bindings(cx, def, arm.pat)?;
        let arm_body = peel_blocks(arm.body);
        let message = if let Some(call) = root_macro_call_first_node(cx, arm_body) {
            if !tcx.is_diagnostic_item(sym::write_macro, call.def_id) {
                return None;
            }
            let parsed = write_call::parse(&snippet_opt(cx, call.span)?)?;
            if parsed.target != formatter.as_str() {
                return None;
            }
            parsed.rewrite(|name| bindings.get(name).cloned())?
        } else if let ExprKind::MethodCall(method, receiver, [text], _) = arm_body.kind
            && method.ident.name.as_str() == "write_str"
            && local(receiver) == Some(formatter_id)
            && let ExprKind::Lit(lit) = text.kind
            && let LitKind::Str(text, _) = lit.node
        {
            write_call::literal_message(text.as_str())
        } else {
            return None;
        };
        if messages.insert(index, message).is_some() {
            return None;
        }
    }

    let attrs = enum_def
        .variants
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let message = messages.get(&index)?;
            let indent = crate::util::indent(cx.sess().source_map(), variant.span);
            Some((variant.span.shrink_to_lo(), format!("#[display(\"{message}\")]\n{indent}")))
        })
        .collect::<Option<Vec<_>>>()?;

    let error_impl = error_impl(cx, adt)?;
    Some(DisplayPlan {
        impl_id,
        impl_span: impl_item.span,
        attrs,
        error_impl,
        under_sugared: under_sugared(cx, adt),
    })
}

/// The one local impl of `trait_id` for `adt`.
fn single_impl(cx: &LateContext<'_>, trait_id: rustc_hir::def_id::DefId, adt: LocalDefId) -> Option<LocalDefId> {
    let mut impls = impls_for(cx, trait_id, adt);
    let first = impls.next()?;
    impls.next().is_none().then_some(first)
}

pub fn impls_for<'a>(
    cx: &'a LateContext<'_>,
    trait_id: rustc_hir::def_id::DefId,
    adt: LocalDefId,
) -> impl Iterator<Item = LocalDefId> + 'a {
    cx.tcx
        .all_local_trait_impls(())
        .get(&trait_id)
        .into_iter()
        .flatten()
        .copied()
        .filter(move |&impl_id| {
            cx.tcx
                .type_of(impl_id)
                .instantiate_identity()
                .skip_norm_wip()
                .ty_adt_def()
                .is_some_and(|def| def.did() == adt.to_def_id())
        })
}

/// `Some(Some(span))` for an empty hand-written `impl Error`, `Some(None)` for no `Error` impl
/// where the enum is `Debug` (so the generated one compiles), `None` when the conversion would
/// lose or conflict with an `Error` impl.
#[expect(clippy::option_option)]
fn error_impl(cx: &LateContext<'_>, adt: LocalDefId) -> Option<Option<Span>> {
    let tcx = cx.tcx;
    let error = tcx.get_diagnostic_item(sym::Error)?;
    let mut impls = impls_for(cx, error, adt);
    match (impls.next(), impls.next()) {
        (None, _) => {
            let debug = tcx.get_diagnostic_item(sym::Debug)?;
            let ty = tcx.type_of(adt).instantiate_identity().skip_norm_wip();
            implements_trait(cx, ty, debug, &[]).then_some(None)
        }
        (Some(impl_id), None) => {
            let item = tcx.hir_expect_item(impl_id);
            let ItemKind::Impl(imp) = item.kind else {
                return None;
            };
            (imp.items.is_empty() && is_hand_written(cx, impl_id, item.span, "impl")).then_some(Some(item.span))
        }
        (Some(_), Some(_)) => None,
    }
}

/// Whether `def_id` was defined in the output of `#[rustx::sugared]`.
pub fn under_sugared(cx: &LateContext<'_>, def_id: LocalDefId) -> bool {
    let expn = cx.tcx.expn_that_defined(def_id.to_def_id());
    hir_util::from_rustx(cx, expn)
        && expn
            .expn_data()
            .macro_def_id
            .is_some_and(|id| cx.tcx.item_name(id).as_str() == "sugared")
}

/// The variant an arm's pattern matches, and the field each of its bindings names.
fn variant_bindings(
    cx: &LateContext<'_>,
    def: AdtDef<'_>,
    pat: &hir::Pat<'_>,
) -> Option<(usize, FxHashMap<String, FieldRef>)> {
    let mut bindings = FxHashMap::default();
    let pat = match pat.kind {
        PatKind::Ref(inner, ..) => inner,
        _ => pat,
    };
    let variant_of = |res: Res| -> Option<(usize, &VariantDef)> {
        let id = match res {
            Res::Def(DefKind::Ctor(CtorOf::Variant, _), ctor) => def.variant_index_with_ctor_id(ctor),
            Res::Def(DefKind::Variant, id) => def.variant_index_with_id(id),
            _ => return None,
        };
        Some((id.as_usize(), def.variant(id)))
    };
    let index = match pat.kind {
        PatKind::TupleStruct(ref qpath, pats, dotdot) => {
            let (index, variant) = variant_of(cx.qpath_res(qpath, pat.hir_id))?;
            let gap = variant.fields.len().checked_sub(pats.len())?;
            for (at, field) in pats.iter().enumerate() {
                let position = match dotdot.as_opt_usize() {
                    Some(dots) if at >= dots => at + gap,
                    _ => at,
                };
                bind(field, FieldRef::Index(position), &mut bindings)?;
            }
            index
        }
        PatKind::Struct(ref qpath, fields, _) => {
            let (index, _) = variant_of(cx.qpath_res(qpath, pat.hir_id))?;
            for field in fields {
                bind(field.pat, FieldRef::Named(field.ident.name.to_string()), &mut bindings)?;
            }
            index
        }
        PatKind::Expr(hir::PatExpr { kind: PatExprKind::Path(qpath), hir_id, .. }) => {
            variant_of(cx.qpath_res(qpath, *hir_id))?.0
        }
        _ => return None,
    };
    Some((index, bindings))
}

fn bind(pat: &hir::Pat<'_>, field: FieldRef, bindings: &mut FxHashMap<String, FieldRef>) -> Option<()> {
    match pat.kind {
        PatKind::Wild => Some(()),
        PatKind::Binding(_, _, ident, None) => {
            bindings.insert(ident.name.to_string(), field);
            Some(())
        }
        _ => None,
    }
}
