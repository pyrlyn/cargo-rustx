use clippy_utils::diagnostics::span_lint_and_then;
use rustc_ast::visit::{self, Visitor};
use rustc_ast::{self as ast, Attribute, ExprKind, StructRest, token};
use rustc_errors::Applicability;
use rustc_lint::{EarlyContext, LintContext};

use crate::attrs::{self, Imports, Macro};
use crate::util::whole_lines;

dylint_linting::declare_lint! {
    /// ### What it does
    ///
    /// Flags `#[rustx::sugared]` on an item that holds no sugar `#[sugared]` would apply.
    ///
    /// ### Why is this bad?
    ///
    /// `#[sugared]` passes valid Rust through unchanged, so on an item without `#[data]`,
    /// `#[newtype]`, `#[display("..")]`, `#[from]`, `#[delegate(..)]`, `#[borrows]` or
    /// `#[widen]` it does nothing. It still makes the reader look for sugar that is not there,
    /// and costs a proc-macro expansion of the whole item.
    ///
    /// ### Example
    ///
    /// ```rust,ignore
    /// #[rustx::sugared]
    /// mod shapes {
    ///     pub struct Square(pub f64);
    /// }
    /// ```
    ///
    /// Use instead:
    ///
    /// ```rust,ignore
    /// mod shapes {
    ///     pub struct Square(pub f64);
    /// }
    /// ```
    pub RUSTX_IDLE_SUGARED,
    Warn,
    "`#[rustx::sugared]` on an item without any sugar inside"
}

/// The sugar names `#[sugared]` reads, as single-segment attributes.
const SUGAR: &[&str] = &["data", "newtype", "display", "from", "delegate", "borrows", "widen"];

pub fn check(cx: &EarlyContext<'_>, item: &ast::Item, sugared: &Attribute, imports: &Imports) {
    let mut finder = SugarFinder { imports, found: false, skip: sugared };
    finder.visit_item(item);
    if finder.found {
        return;
    }
    span_lint_and_then(
        cx,
        RUSTX_IDLE_SUGARED,
        sugared.span,
        "`#[sugared]` has nothing to desugar in this item",
        |diag| {
            diag.note(
                "`#[sugared]` applies `#[data]`, `#[newtype]`, `#[display(\"..\")]`, `#[from]`, \
                 `#[delegate(..)]`, `#[borrows]` and `#[widen]`, and none of them is written here",
            );
            diag.span_suggestion(
                whole_lines(cx.sess().source_map(), sugared.span),
                "remove the attribute",
                "",
                Applicability::MachineApplicable,
            );
        },
    );
}

struct SugarFinder<'a> {
    imports: &'a Imports,
    found: bool,
    /// The `#[sugared]` attribute being checked, which is not sugar inside its own item.
    skip: &'a Attribute,
}

impl<'a> Visitor<'a> for SugarFinder<'_> {
    fn visit_attribute(&mut self, attr: &'a Attribute) {
        if attr.id == self.skip.id {
            return;
        }
        // A qualified rustx macro inside counts as sugar: `rustx_macro_in_sugared` asks for it to
        // be written bare, which only works while `#[sugared]` stays.
        let bare = SUGAR.iter().any(|name| attrs::is_bare(attr, name));
        let rustx = attrs::rustx_macro(attr, self.imports).is_some_and(|(mac, _)| mac != Macro::Sugared);
        self.found |= bare || rustx;
    }

    fn visit_expr(&mut self, expr: &'a ast::Expr) {
        // `S { a: 1, .. }` (field defaults) and `S { name: "lit" }` (string literals into `String`
        // fields) are sugar `#[sugared]` may rewrite; counting every such literal keeps this lint
        // from calling an item idle that is not.
        if let ExprKind::Struct(se) = &expr.kind {
            let rest = matches!(se.rest, StructRest::Rest(_));
            let literal = se.fields.iter().any(|field| {
                matches!(
                    &field.expr.kind,
                    ExprKind::Lit(token::Lit { kind: token::LitKind::Str | token::LitKind::StrRaw(_), .. })
                )
            });
            self.found |= rest || literal;
        }
        visit::walk_expr(self, expr);
    }
}
