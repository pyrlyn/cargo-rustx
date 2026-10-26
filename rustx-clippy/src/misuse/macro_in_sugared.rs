use clippy_utils::diagnostics::span_lint_and_then;
use rustc_ast::{self as ast, Attribute};
use rustc_errors::Applicability;
use rustc_lint::{EarlyContext, LintContext};

use crate::attrs::{self, Imports, Macro, Spelling};
use crate::util::whole_lines;

dylint_linting::declare_lint! {
    /// ### What it does
    ///
    /// Flags a rustx attribute macro inside an item that `#[rustx::sugared]` already covers:
    /// `#[rustx::data]` or `#[rustx::newtype]` written with their path, and `#[display]`,
    /// `#[delegate]` or `#[sugared]` in any spelling.
    ///
    /// ### Why is this bad?
    ///
    /// `#[sugared]` applies the bare sugar names itself, with the whole item in view. A
    /// qualified `#[rustx::data]` or `#[rustx::newtype]` is not one of them: it stays behind and
    /// expands later as a separate macro that sees only its own struct, so
    /// `#[rustx::newtype(validate = f)]` cannot find `f` although `f` is in the module.
    /// `#[display]` and `#[delegate]` on the type are markers only the standalone macros need:
    /// `#[sugared]` consumes the variants' `#[display("..")]` and the fields' `#[delegate(..)]`
    /// first, and the leftover marker then fails with "needs an enum whose variants carry
    /// `#[display(\"..\")]`". A nested `#[sugared]` finds nothing left to do.
    ///
    /// ### Example
    ///
    /// ```rust,ignore
    /// #[rustx::sugared]
    /// mod model {
    ///     #[rustx::newtype(deref, validate = check)]
    ///     pub struct Email(String);
    ///
    ///     #[rustx::display]
    ///     #[derive(Debug)]
    ///     pub enum Error {
    ///         #[display("bad email")]
    ///         Email,
    ///     }
    ///     # fn check(_: &String) -> Result<(), &'static str> { Ok(()) }
    /// }
    /// ```
    ///
    /// Use instead:
    ///
    /// ```rust,ignore
    /// #[rustx::sugared]
    /// mod model {
    ///     #[newtype(deref, validate = check)]
    ///     pub struct Email(String);
    ///
    ///     #[derive(Debug)]
    ///     pub enum Error {
    ///         #[display("bad email")]
    ///         Email,
    ///     }
    ///     # fn check(_: &String) -> Result<(), &'static str> { Ok(()) }
    /// }
    /// ```
    pub RUSTX_MACRO_IN_SUGARED,
    Warn,
    "a rustx attribute macro inside an item `#[rustx::sugared]` already covers"
}

/// Checks the attributes of `item`, which `#[sugared]` covers; `cover` is that `#[sugared]`
/// when it sits on `item` itself.
pub fn check(cx: &EarlyContext<'_>, item: &ast::Item, cover: Option<&Attribute>, imports: &Imports) {
    for attr in &item.attrs {
        if cover.is_some_and(|cover| cover.id == attr.id) {
            continue;
        }
        let Some((mac, spelling)) = attrs::rustx_macro(attr, imports) else {
            continue;
        };
        match (mac, spelling) {
            // A bare imported `#[data]` / `#[newtype]` is exactly what `#[sugared]` reads.
            (Macro::Data | Macro::Newtype, Spelling::Imported) => {}
            (Macro::Data | Macro::Newtype, Spelling::Qualified) => qualified(cx, attr, mac),
            (Macro::Display | Macro::Delegate | Macro::Sugared, _) => marker(cx, attr, mac),
        }
    }
}

fn qualified(cx: &EarlyContext<'_>, attr: &Attribute, mac: Macro) {
    let name = mac.name();
    let Some(path) = attrs::path_span(attr) else {
        return;
    };
    span_lint_and_then(
        cx,
        RUSTX_MACRO_IN_SUGARED,
        attr.span,
        format!("`#[rustx::{name}]` inside `#[sugared]` expands as a separate macro"),
        |diag| {
            diag.note(format!(
                "`#[sugared]` applies a bare `#[{name}]` with the whole item in view; the \
                 qualified path is left for the standalone macro, which sees only this struct"
            ));
            diag.span_suggestion(
                path,
                "let `#[sugared]` apply it",
                name,
                Applicability::MachineApplicable,
            );
        },
    );
}

fn marker(cx: &EarlyContext<'_>, attr: &Attribute, mac: Macro) {
    let name = mac.name();
    let why = match mac {
        Macro::Display => {
            "`#[sugared]` already turns the variants' `#[display(\"..\")]` and `#[from]` into \
             impls; the leftover marker then finds no `#[display(\"..\")]` and fails"
        }
        Macro::Delegate => {
            "`#[sugared]` already forwards the fields' `#[delegate(..)]`; the leftover marker \
             then finds no `#[delegate(..)]` field and fails"
        }
        _ => "the outer `#[sugared]` has already applied all sugar inside this item",
    };
    span_lint_and_then(
        cx,
        RUSTX_MACRO_IN_SUGARED,
        attr.span,
        format!("`#[{name}]` inside `#[sugared]` is redundant"),
        |diag| {
            diag.note(why);
            diag.span_suggestion(
                whole_lines(cx.sess().source_map(), attr.span),
                "remove it",
                "",
                Applicability::MachineApplicable,
            );
        },
    );
}
