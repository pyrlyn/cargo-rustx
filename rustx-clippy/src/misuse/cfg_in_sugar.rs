use clippy_utils::diagnostics::span_lint_and_then;
use rustc_ast::{self as ast, Attribute};
use rustc_lint::EarlyContext;
use rustc_span::Span;

use crate::attrs;
use crate::misuse::Applied;

dylint_linting::declare_lint! {
    /// ### What it does
    ///
    /// Flags `#[cfg(..)]` on a field or variant that rustx sugar generates code for: any field of
    /// a `#[data]` or `#[newtype]` struct, a `#[delegate(..)]` field, any variant of an enum with
    /// `#[display("..")]` variants, and a `#[from]` variant.
    ///
    /// ### Why is this bad?
    ///
    /// rustc hands an attribute macro its item before evaluating the `#[cfg]`s inside it, and the
    /// sugar does not copy them onto what it generates. `new`, the getters and the builder of
    /// `#[data]`, the forwarding impl of `#[delegate]`, the `match` of `#[display]` and the
    /// `From` impl of `#[from]` name the field or variant unconditionally. The crate builds while
    /// the cfg holds and fails with "no field" or "no variant" errors as soon as it does not, for
    /// example in a build without that feature.
    ///
    /// There is no sugar that keeps the cfg, so this lint has no automatic fix.
    ///
    /// ### Example
    ///
    /// ```rust,ignore
    /// #[rustx::data]
    /// pub struct Config {
    ///     name: String,
    ///     #[cfg(feature = "tls")]
    ///     cert: std::path::PathBuf,
    /// }
    /// ```
    ///
    /// Use instead: keep the field unconditional and make its type depend on the cfg,
    ///
    /// ```rust,ignore
    /// #[cfg(feature = "tls")]
    /// pub type Cert = std::path::PathBuf;
    /// #[cfg(not(feature = "tls"))]
    /// pub type Cert = ();
    ///
    /// #[rustx::data]
    /// pub struct Config {
    ///     name: String,
    ///     cert: Cert,
    /// }
    /// ```
    ///
    /// or drop the sugar from this type and write the constructor and accessors by hand, each with
    /// the same `#[cfg]` as its field.
    pub RUSTX_CFG_IN_SUGAR,
    Warn,
    "`#[cfg]` on a field or variant that rustx sugar generates code for"
}

pub fn check(cx: &EarlyContext<'_>, item: &ast::Item, applied: Applied) {
    match &item.kind {
        ast::ItemKind::Struct(_, _, data) => {
            for field in data.fields() {
                let sugar = if applied.data {
                    Some("#[data]")
                } else if applied.newtype {
                    Some("#[newtype]")
                } else if applied.delegate && attrs::has_bare(&field.attrs, "delegate") {
                    Some("#[delegate(..)]")
                } else {
                    None
                };
                if let (Some(sugar), Some(cfg)) = (sugar, cfg_attr(&field.attrs)) {
                    let what = field.ident.map_or_else(|| "this field".to_owned(), |name| format!("field `{name}`"));
                    lint(cx, cfg, sugar, &what, Gated::Field);
                }
            }
        }
        ast::ItemKind::Enum(_, _, def) if applied.display => {
            let derived: Vec<String> = attrs::derived(&item.attrs)
                .into_iter()
                .map(|(name, _)| name.to_string())
                .collect();
            let derives = |name: &str| derived.iter().any(|d| d == name);
            // An enum that derives `Display` keeps its `#[display]` for that derive, and one that
            // derives `Error` or `From` keeps its `#[from]`; rustx generates nothing for them.
            let display = !derives("Display") && def.variants.iter().any(|v| attrs::has_bare(&v.attrs, "display"));
            let from_derived = derives("Error") || derives("From");
            for variant in &def.variants {
                let from = !from_derived
                    && variant.data.fields().iter().any(|f| attrs::has_bare(&f.attrs, "from"));
                let sugar = if display {
                    "#[display(\"..\")]"
                } else if from {
                    "#[from]"
                } else {
                    continue;
                };
                let what = format!("variant `{}`", variant.ident);
                if let Some(cfg) = cfg_attr(&variant.attrs) {
                    lint(cx, cfg, sugar, &what, Gated::Variant);
                }
                if from && let Some(cfg) = variant.data.fields().iter().find_map(|f| cfg_attr(&f.attrs)) {
                    lint(cx, cfg, "#[from]", &what, Gated::Field);
                }
            }
        }
        _ => {}
    }
}

#[derive(Clone, Copy)]
enum Gated {
    Field,
    Variant,
}

fn cfg_attr(attrs: &[Attribute]) -> Option<Span> {
    attrs.iter().find(|attr| attrs::is_bare(attr, "cfg")).map(|attr| attr.span)
}

fn lint(cx: &EarlyContext<'_>, cfg: Span, sugar: &str, what: &str, gated: Gated) {
    span_lint_and_then(
        cx,
        RUSTX_CFG_IN_SUGAR,
        cfg,
        format!("`#[cfg]` on {what} is not carried into the code `{sugar}` generates"),
        |diag| {
            diag.note(format!(
                "the generated code names {what} unconditionally, so the crate fails to build \
                 whenever this cfg is off"
            ));
            match gated {
                Gated::Field => diag.help(
                    "keep the field and gate its type instead (`#[cfg(..)] type T = ..;` with a \
                     `#[cfg(not(..))]` fallback such as `()`), or write the generated methods by \
                     hand with the same `#[cfg]`",
                ),
                Gated::Variant => diag.help(
                    "keep the variant unconditional, or write `Display` / `From` by hand with the \
                     same `#[cfg]` on the arm or impl",
                ),
            };
        },
    );
}
