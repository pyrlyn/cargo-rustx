use clippy_utils::diagnostics::span_lint_hir_and_then;
use rustc_errors::Applicability;
use rustc_hir::def::{CtorOf, DefKind, Res};
use rustc_hir::def_id::LocalDefId;
use rustc_hir::{self as hir, ExprKind, ImplItemKind, ItemKind, PatKind, QPath};
use rustc_lint::LateContext;
use rustc_span::{MultiSpan, Span, sym};

use crate::convert::manual_display::{self, impls_for, under_sugared};
use crate::hir_util::{self, is_hand_written, local, peel_blocks};

dylint_linting::declare_lint! {
    /// ### What it does
    ///
    /// Flags a hand-written `impl From<T> for Enum` whose body only wraps the value in a
    /// one-field tuple variant of type `T`: what `#[from]` on that variant's field generates.
    ///
    /// ### Why is this bad?
    ///
    /// The impl repeats the variant and its type in a separate item, one impl per variant; an
    /// error enum with five sources has five of them. `Io(#[from] io::Error)` says the same on
    /// the variant.
    ///
    /// The fix adds `#[from]` to the field, removes the impl and, unless the enum already sits
    /// under `#[rustx::display]` or `#[rustx::sugared]`, adds `#[rustx::sugared]` to the enum
    /// (or relies on the `#[rustx::display]` that `rustx_manual_display` suggests for the same
    /// enum). It is machine-applicable when the crate depends on `rustx`.
    ///
    /// ### Example
    ///
    /// ```rust,ignore
    /// pub enum Source {
    ///     Io(std::io::Error),
    ///     Utf8(std::str::Utf8Error),
    /// }
    ///
    /// impl From<std::io::Error> for Source {
    ///     fn from(e: std::io::Error) -> Self {
    ///         Self::Io(e)
    ///     }
    /// }
    /// ```
    ///
    /// Use instead:
    ///
    /// ```rust,ignore
    /// #[rustx::sugared]
    /// pub enum Source {
    ///     Io(#[from] std::io::Error),
    ///     Utf8(std::str::Utf8Error),
    /// }
    /// ```
    pub RUSTX_MANUAL_FROM,
    Warn,
    "a hand-written `From` impl that `#[from]` on an enum variant generates"
}

pub fn check(cx: &LateContext<'_>, item: &hir::Item<'_>) {
    let ItemKind::Enum(_, generics, enum_def) = item.kind else {
        return;
    };
    let adt = item.owner_id.def_id;
    if !generics.params.is_empty() || !is_hand_written(cx, adt, item.span, "enum") || derives_from_or_error(cx, adt) {
        return;
    }
    let Some(from) = cx.tcx.get_diagnostic_item(sym::From) else {
        return;
    };
    let found: Vec<(LocalDefId, Span, Span)> = impls_for(cx, from, adt)
        .filter_map(|impl_id| wrapping_variant(cx, impl_id, adt, enum_def))
        .collect();
    let Some(&(first, ..)) = found.first() else {
        return;
    };

    let mut edits = Vec::new();
    let mut spans = MultiSpan::new();
    let mut exact = true;
    for &(impl_id, impl_span, field) in &found {
        spans.push_primary_span(impl_span);
        edits.push((field.shrink_to_lo(), "#[from] ".to_owned()));
        edits.push(hir_util::remove_with_attrs(cx, cx.tcx.local_def_id_to_hir_id(impl_id), impl_span));
        // Attributes on the impl (docs, `#[allow]`, `#[cfg]`) would be lost with it.
        exact &= cx.tcx.hir_attrs(cx.tcx.local_def_id_to_hir_id(impl_id)).is_empty();
    }
    spans.push_span_label(item.span.shrink_to_lo(), "for this enum");

    let display = manual_display::plan(cx, adt);
    let covered = hir_util::has_rustx_impl(cx, adt) || under_sugared(cx, adt);
    let mut note = None;
    if covered {
        // `#[rustx::display]` or `#[rustx::sugared]` already reads `#[from]`.
    } else if let Some(display) = &display {
        exact &= display.exact();
        note = Some(
            "the `#[rustx::display]` that `rustx_manual_display` suggests for this enum also reads \
             `#[from]`; apply both together",
        );
    } else {
        edits.push(hir_util::insert_attr(cx, item.span, "#[rustx::sugared]"));
    }
    let applicability = if note.is_some() && !exact {
        Applicability::MaybeIncorrect
    } else {
        hir_util::applicability(cx, exact)
    };
    let message = if found.len() == 1 {
        "this `From` impl can be converted to `#[from]` on the variant".to_owned()
    } else {
        format!("these {} `From` impls can be converted to `#[from]` on the variants", found.len())
    };
    span_lint_hir_and_then(
        cx,
        RUSTX_MANUAL_FROM,
        cx.tcx.local_def_id_to_hir_id(first),
        spans,
        message,
        |diag| {
            if let Some(note) = note {
                diag.note(note);
            }
            hir_util::note_dependency(cx, diag);
            diag.multipart_suggestion("mark the variant fields with `#[from]`", edits, applicability);
        },
    );
}

/// For an `impl From<T> for Enum` that only wraps its argument in a one-field tuple variant of
/// type `T`: the impl, its span and the span of that variant's field.
fn wrapping_variant(
    cx: &LateContext<'_>,
    impl_id: LocalDefId,
    adt: LocalDefId,
    enum_def: hir::EnumDef<'_>,
) -> Option<(LocalDefId, Span, Span)> {
    let tcx = cx.tcx;
    let item = tcx.hir_expect_item(impl_id);
    let ItemKind::Impl(imp) = item.kind else {
        return None;
    };
    if !imp.generics.params.is_empty() || !is_hand_written(cx, impl_id, item.span, "impl") {
        return None;
    }
    let [from] = imp.items else {
        return None;
    };
    let ImplItemKind::Fn(_, body_id) = tcx.hir_impl_item(*from).kind else {
        return None;
    };
    let body = tcx.hir_body(body_id);
    let [param] = body.params else {
        return None;
    };
    let PatKind::Binding(_, value, _, None) = param.pat.kind else {
        return None;
    };
    let ExprKind::Call(callee, [arg]) = peel_blocks(body.value).kind else {
        return None;
    };
    let ExprKind::Path(ref qpath) = callee.kind else {
        return None;
    };
    let Res::Def(DefKind::Ctor(CtorOf::Variant, _), ctor) = cx.qpath_res(qpath, callee.hir_id) else {
        return None;
    };
    if local(arg) != Some(value) || !matches!(qpath, QPath::Resolved(..) | QPath::TypeRelative(..)) {
        return None;
    }
    let def = tcx.adt_def(adt);
    let index = def.variant_index_with_ctor_id(ctor);
    let variant = def.variant(index);
    let [field] = &variant.fields.raw[..] else {
        return None;
    };
    let source = tcx.impl_trait_ref(impl_id).instantiate_identity().skip_norm_wip().args.type_at(1);
    if tcx.type_of(field.did).instantiate_identity().skip_norm_wip() != source {
        return None;
    }
    let hir_variant = enum_def.variants.get(index.as_usize())?;
    let [hir_field] = hir_variant.data.fields() else {
        return None;
    };
    Some((impl_id, item.span, hir_field.span))
}

/// An enum that derives `Error` or `From` (thiserror, derive_more) keeps its `#[from]` for that
/// derive: rustx generates nothing for it, and the derive generates its own impl.
fn derives_from_or_error(cx: &LateContext<'_>, adt: LocalDefId) -> bool {
    let tcx = cx.tcx;
    [sym::Error, sym::From].into_iter().filter_map(|name| tcx.get_diagnostic_item(name)).any(|trait_id| {
        impls_for(cx, trait_id, adt).any(|impl_id| {
            matches!(
                tcx.expn_that_defined(impl_id.to_def_id()).expn_data().kind,
                rustc_span::hygiene::ExpnKind::Macro(rustc_span::hygiene::MacroKind::Derive, _)
            )
        })
    })
}
