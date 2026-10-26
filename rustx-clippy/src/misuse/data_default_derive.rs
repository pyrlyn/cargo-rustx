use clippy_utils::diagnostics::span_lint_and_then;
use rustc_ast::{self as ast};
use rustc_lint::EarlyContext;
use rustc_span::MultiSpan;

use crate::attrs;
use crate::misuse::Applied;

dylint_linting::declare_lint! {
    /// ### What it does
    ///
    /// Flags a `#[data]` struct that derives `Default` while some of its fields carry
    /// `#[default(expr)]`.
    ///
    /// ### Why is this bad?
    ///
    /// `#[default(expr)]` is read by `#[data]` alone: `new` leaves the field out and fills in
    /// `expr`, and so does the builder. `#[derive(Default)]` never sees it (the macro removes it
    /// first) and fills the field with its type's `Default`. The type then has two defaults that
    /// disagree: `User::new(..).age() == 18` but `User::default().age() == 0`.
    ///
    /// ### Example
    ///
    /// ```rust,ignore
    /// #[rustx::data]
    /// #[derive(Default)]
    /// pub struct User {
    ///     name: String,
    ///     #[default(18)]
    ///     age: u8,
    /// }
    /// ```
    ///
    /// Use instead: write `Default` by hand so it uses the same value,
    ///
    /// ```rust,ignore
    /// #[rustx::data]
    /// pub struct User {
    ///     name: String,
    ///     #[default(18)]
    ///     age: u8,
    /// }
    ///
    /// impl Default for User {
    ///     fn default() -> Self {
    ///         Self { name: String::new(), age: 18 }
    ///     }
    /// }
    /// ```
    ///
    /// or, in `rustx::sugar!` or a `.rustx` file, write the field default `age: u8 = 18`, which
    /// generates the `Default` impl that `new` and the builder share.
    pub RUSTX_DATA_DEFAULT_DERIVE,
    Warn,
    "`#[derive(Default)]` on a `#[data]` struct ignores its `#[default(..)]` fields"
}

pub fn check(cx: &EarlyContext<'_>, item: &ast::Item, applied: Applied) {
    let ast::ItemKind::Struct(_, _, data) = &item.kind else {
        return;
    };
    if !applied.data {
        return;
    }
    let Some((_, derive)) = attrs::derived(&item.attrs)
        .into_iter()
        .find(|(name, _)| name.as_str() == "Default")
    else {
        return;
    };
    let defaults: Vec<_> = data
        .fields()
        .iter()
        .filter_map(|field| {
            let attr = field.attrs.iter().find(|attr| attrs::is_bare(attr, "default"))?;
            Some((attr.span, field.ident?))
        })
        .collect();
    if defaults.is_empty() {
        return;
    }
    let mut spans = MultiSpan::from_span(derive);
    for (span, name) in &defaults {
        spans.push_span_label(*span, format!("`new` and the builder default `{name}` to this"));
    }
    span_lint_and_then(
        cx,
        RUSTX_DATA_DEFAULT_DERIVE,
        spans,
        "`#[derive(Default)]` ignores the `#[default(..)]` values of this `#[data]` struct",
        |diag| {
            diag.note(
                "only `#[data]` reads `#[default(..)]`; the derived `Default` fills these fields \
                 with their type's default, so `Default::default()` and `new` disagree",
            );
            diag.help(
                "drop the derive and write `impl Default` with the same values, or use the field \
                 default `field: T = expr` inside `rustx::sugar!`, which generates one `Default` \
                 for `new`, the builder and `Default::default()`",
            );
        },
    );
}
