//! Misuse of rustx's attribute sugar, found before macro expansion.
//!
//! These lints run as a pre-expansion pass: after expansion the attributes are gone, and with
//! them what the user wrote. The pass resolves nothing, so it recognises rustx by spelling (see
//! [`crate::attrs`]).

pub mod cfg_in_sugar;
pub mod data_default_derive;
pub mod idle_sugared;
pub mod macro_in_sugared;

use rustc_ast as ast;
use rustc_lint::{EarlyContext, EarlyLintPass};

use crate::attrs::{self, Imports, Macro};

/// Which rustx sugar applies to an item, whether through its own attribute macro or through an
/// enclosing `#[sugared]`.
#[derive(Clone, Copy, Default)]
pub struct Applied {
    pub data: bool,
    pub newtype: bool,
    /// The variants' `#[display("..")]` and `#[from]` are read.
    pub display: bool,
    /// The fields' `#[delegate(..)]` are read.
    pub delegate: bool,
}

impl Applied {
    fn of(item: &ast::Item, imports: &Imports, covered: bool) -> Self {
        let has = |mac: Macro| attrs::find_macro(&item.attrs, imports, mac).is_some();
        let bare = |name: &str| covered && attrs::has_bare(&item.attrs, name);
        Self {
            data: has(Macro::Data) || bare("data"),
            newtype: has(Macro::Newtype) || bare("newtype"),
            display: has(Macro::Display) || covered,
            delegate: has(Macro::Delegate) || covered,
        }
    }
}

/// The pre-expansion pass behind every lint of this module.
#[derive(Default)]
pub struct Misuse {
    imports: Imports,
    /// The items carrying `#[sugared]` the walk is inside of, innermost last, by address: node ids
    /// are not assigned before expansion.
    sugared: Vec<*const ast::Item>,
}

dylint_linting::impl_lint_pass!(Misuse => [
    idle_sugared::RUSTX_IDLE_SUGARED,
    macro_in_sugared::RUSTX_MACRO_IN_SUGARED,
    cfg_in_sugar::RUSTX_CFG_IN_SUGAR,
    data_default_derive::RUSTX_DATA_DEFAULT_DERIVE,
]);

impl EarlyLintPass for Misuse {
    fn check_item(&mut self, cx: &EarlyContext<'_>, item: &ast::Item) {
        self.imports.record(item);
        let own = attrs::find_macro(&item.attrs, &self.imports, Macro::Sugared).map(|(attr, _)| attr);
        let outer = !self.sugared.is_empty();
        if let Some(sugared) = own {
            if !outer {
                idle_sugared::check(cx, item, sugared, &self.imports);
            }
            self.sugared.push(item);
        }
        let covered = !self.sugared.is_empty();
        if covered {
            // An outer `#[sugared]` makes this item's own `#[sugared]` redundant too.
            macro_in_sugared::check(cx, item, own.filter(|_| !outer), &self.imports);
        }
        let applied = Applied::of(item, &self.imports, covered);
        cfg_in_sugar::check(cx, item, applied);
        data_default_derive::check(cx, item, applied);
    }

    fn check_item_post(&mut self, _: &EarlyContext<'_>, item: &ast::Item) {
        if self.sugared.last().is_some_and(|last| std::ptr::eq(*last, item)) {
            self.sugared.pop();
        }
    }
}
