//! `Rc::new(RefCell::new(e))` back to `rc e`: the reverse of `passes/rc_values.rs` (problem 16).
//!
//! The pass emits `::std::rc::Rc::new(::std::cell::RefCell::new(e))`; any spelling of those two
//! paths that `std_paths` resolves to the same items matches (`Rc::new` after
//! `use std::rc::Rc`, `rc::Rc::new` after `use std::rc`, `std::rc::Rc::new`). A path with
//! generic arguments (`Rc::<RefCell<T>>::new`) does not: `rc e` cannot write them.
//!
//! `rc e` takes everything up to the next `,` or `;` as its value, so a call converts only where
//! one of those or a closing delimiter follows it: a `let` value, a statement or block tail, a
//! call or method argument, an array or tuple element, a parenthesised expression, the value of
//! `return`, `break`, an assignment, a closure body or a match arm. Not as a method receiver
//! (`Rc::new(..).clone()`), an operand or a cast, and not as a struct field value, where
//! `f: rc x` reads as a type. The value itself must make `rc` sugar, which the pass decides: it
//! must not start with `&`, `-`, `*`, `|`, `(` or `if`, for example. A comment between the calls'
//! parentheses would be lost, so it keeps the call as written.
//!
//! A value holding another such call converts inside out (`rc rc 1`); a `var` or turbofish edit
//! inside a value is dropped by the driver, since the `rc` edit covers it. On a `let` binding,
//! `rc e` makes `rc_borrows` borrow on field accesses and value-method calls of the name
//! (problem 16), which changes plain code such as `x.replace(v)`; the driver's check refuses
//! those edits, so the rule does not repeat that analysis.

use std::ops::Range;

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote};
use syn::visit::{self, Visit};
use syn::{
    Arm, Expr, ExprArray, ExprAssign, ExprBreak, ExprCall, ExprClosure, ExprMethodCall, ExprParen,
    ExprReturn, ExprTuple, Local, Stmt,
};

use super::std_paths::StdPaths;
use super::{Edit, ReverseRule, apply, leaves};

pub(crate) struct RcValues;

impl ReverseRule for RcValues {
    fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit> {
        let paths = StdPaths::of(file);
        let mut finder = Finder {
            source,
            rc_new: paths.normalise(quote!(::std::rc::Rc::new)),
            ref_cell_new: paths.normalise(quote!(::std::cell::RefCell::new)),
            paths: &paths,
            edits: Vec::new(),
        };
        finder.visit_file(file);
        finder.edits
    }
}

struct Finder<'a> {
    source: &'a str,
    paths: &'a StdPaths,
    rc_new: Vec<String>,
    ref_cell_new: Vec<String>,
    edits: Vec<Edit>,
}

impl Finder<'_> {
    /// Visits an expression that sits where `rc e` would end, converting it if it matches.
    fn value(&mut self, expr: &Expr) {
        match self.convert(expr) {
            Some(edit) => self.edits.push(edit),
            None => self.visit_expr(expr),
        }
    }

    fn convert(&self, expr: &Expr) -> Option<Edit> {
        let outer = self.call_of(expr, &self.rc_new)?;
        let inner = self.call_of(&outer.args[0], &self.ref_cell_new)?;
        let value = &inner.args[0];
        let (first, last) = bounds(value.to_token_stream())?;
        let (start, end) = (first.byte_range().start, last.byte_range().end);
        let gaps = [
            outer.paren_token.span.open().byte_range().end
                ..bounds(inner.func.to_token_stream())?.0.byte_range().start,
            inner.paren_token.span.open().byte_range().end..start,
            end..inner.paren_token.span.close().byte_range().start,
            inner.paren_token.span.close().byte_range().end
                ..outer.paren_token.span.close().byte_range().start,
        ];
        let blank = |gap: Range<usize>| {
            self.source
                .get(gap)
                .is_some_and(|gap| gap.trim().is_empty())
        };
        if !gaps.into_iter().all(blank) || !reads_back(value) {
            return None;
        }

        // Nested calls inside the value convert as well, edited relative to its text.
        let mut nested = Finder {
            source: self.source,
            paths: self.paths,
            rc_new: self.rc_new.clone(),
            ref_cell_new: self.ref_cell_new.clone(),
            edits: Vec::new(),
        };
        nested.value(value);
        nested.edits.sort_by_key(|edit| edit.range.start);
        let nested: Vec<Edit> = nested
            .edits
            .into_iter()
            .map(|edit| Edit {
                range: edit.range.start - start..edit.range.end - start,
                text: edit.text,
            })
            .collect();
        let text = format!("rc {}", apply(self.source.get(start..end)?, &nested));
        let (func_start, _) = bounds(outer.func.to_token_stream())?;
        Some(Edit::between(
            func_start,
            outer.paren_token.span.close(),
            &text,
        ))
    }

    /// `expr` when it calls `func` with one argument, through a plain path.
    fn call_of<'e>(&self, expr: &'e Expr, func: &[String]) -> Option<&'e ExprCall> {
        let Expr::Call(call) = expr else {
            return None;
        };
        let Expr::Path(path) = &*call.func else {
            return None;
        };
        let plain = call.attrs.is_empty()
            && path.attrs.is_empty()
            && path.qself.is_none()
            && call.args.len() == 1
            && !call.args.trailing_punct();
        (plain && self.paths.normalise(path.path.to_token_stream()) == func).then_some(call)
    }
}

/// The spans of the first and last token of `tokens`.
pub(super) fn bounds(tokens: TokenStream) -> Option<(Span, Span)> {
    let mut all = Vec::new();
    leaves(tokens, &mut all);
    Some((all.first()?.1, all.last()?.1))
}

/// Whether the token passes read `rc value` as the sugar for exactly this value.
fn reads_back(value: &Expr) -> bool {
    let sugar = quote!(fn f() { let _ = rc #value; });
    let plain = quote!(fn f() { let _ = ::std::rc::Rc::new(::std::cell::RefCell::new(#value)); });
    let expanded = crate::try_expand(sugar).map(|file| file.into_token_stream().to_string());
    syn::parse2::<syn::File>(plain).is_ok_and(|plain| {
        expanded.is_ok_and(|expanded| expanded == plain.into_token_stream().to_string())
    })
}

impl<'ast> Visit<'ast> for Finder<'_> {
    fn visit_local(&mut self, local: &'ast Local) {
        match &local.init {
            Some(init) if init.diverge.is_none() => {
                self.visit_pat(&local.pat);
                self.value(&init.expr);
            }
            _ => visit::visit_local(self, local),
        }
    }

    fn visit_stmt(&mut self, stmt: &'ast Stmt) {
        match stmt {
            Stmt::Expr(expr, _) => self.value(expr),
            stmt => visit::visit_stmt(self, stmt),
        }
    }

    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        self.visit_expr(&call.func);
        call.args.iter().for_each(|arg| self.value(arg));
    }

    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        self.visit_expr(&call.receiver);
        call.args.iter().for_each(|arg| self.value(arg));
    }

    fn visit_expr_array(&mut self, array: &'ast ExprArray) {
        array.elems.iter().for_each(|elem| self.value(elem));
    }

    fn visit_expr_tuple(&mut self, tuple: &'ast ExprTuple) {
        tuple.elems.iter().for_each(|elem| self.value(elem));
    }

    fn visit_expr_paren(&mut self, paren: &'ast ExprParen) {
        self.value(&paren.expr);
    }

    fn visit_expr_return(&mut self, ret: &'ast ExprReturn) {
        if let Some(expr) = &ret.expr {
            self.value(expr);
        }
    }

    fn visit_expr_break(&mut self, brk: &'ast ExprBreak) {
        if let Some(expr) = &brk.expr {
            self.value(expr);
        }
    }

    fn visit_expr_assign(&mut self, assign: &'ast ExprAssign) {
        self.visit_expr(&assign.left);
        self.value(&assign.right);
    }

    fn visit_expr_closure(&mut self, closure: &'ast ExprClosure) {
        self.value(&closure.body);
    }

    fn visit_arm(&mut self, arm: &'ast Arm) {
        // The pattern holds the guard.
        self.visit_pat(&arm.pat);
        self.value(&arm.body);
    }
}

#[cfg(test)]
mod tests {
    use super::RcValues;
    use crate::sugar::tests::assert_sugar;
    use crate::sugar::{ReverseRule, sugar};

    const USES: &str = "use std::cell::RefCell;\nuse std::rc::Rc;\n";

    fn with_uses(body: &str) -> String {
        format!("{USES}{body}")
    }

    #[test]
    fn rc_ref_cell_new_becomes_rc_in_value_positions() {
        assert_sugar(
            &with_uses(
                "fn f(v: Vec<u8>) -> Rc<RefCell<u8>> {\n    let a = Rc::new(RefCell::new(v));\n    \
                 g(Rc::new(RefCell::new(1)), [Rc::new(RefCell::new(2))]);\n    \
                 let c = || Rc::new(RefCell::new(3));\n    \
                 match 0 { 0 => Rc::new(RefCell::new(4)), _ => Rc::new(RefCell::new(5)) }\n}\n",
            ),
            &with_uses(
                "fn f(v: Vec<u8>) -> Rc<RefCell<u8>> {\n    let a = rc v;\n    \
                 g(rc 1, [rc 2]);\n    \
                 let c = || rc 3;\n    \
                 match 0 { 0 => rc 4, _ => rc 5 }\n}\n",
            ),
        );
    }

    #[test]
    fn every_equivalent_spelling_converts_and_nested_calls_convert_inside_out() {
        assert_sugar(
            "use std::rc;\nfn f() {\n    let a = rc::Rc::new(std::cell::RefCell::new(1));\n    \
             let b = ::std::rc::Rc::new(::core::cell::RefCell::new(Node::default()));\n    \
             let c = rc::Rc::new(::std::cell::RefCell::new(rc::Rc::new(::std::cell::RefCell::new(vec![1]))));\n}\n",
            "use std::rc;\nfn f() {\n    let a = rc 1;\n    \
             let b = rc Node::default();\n    \
             let c = rc rc vec![1];\n}\n",
        );
    }

    #[test]
    fn handle_methods_on_the_binding_keep_the_conversion() {
        assert_sugar(
            &with_uses(
                "fn f() -> usize {\n    let x = Rc::new(RefCell::new(Vec::new()));\n    \
                 x.borrow_mut().push(1);\n    let y = x.clone();\n    \
                 let n = Rc::strong_count(&y);\n    x.borrow().len() + n\n}\n",
            ),
            &with_uses(
                "fn f() -> usize {\n    let x = rc Vec::new();\n    \
                 x.borrow_mut().push(1);\n    let y = x.clone();\n    \
                 let n = Rc::strong_count(&y);\n    x.borrow().len() + n\n}\n",
            ),
        );
    }

    #[test]
    fn a_binding_the_borrows_pass_would_rewrite_stays_plain() {
        // As sugar, `rc_borrows` would read `x.replace(2)` as `x.borrow_mut().replace(2)`.
        let source = with_uses(
            "fn f() -> u8 {\n    let x = Rc::new(RefCell::new(1));\n    x.replace(2)\n}\n",
        );
        let file = syn::parse_file(&source).unwrap();
        assert_eq!(RcValues.edits(&file, &source).len(), 1);
        assert_eq!(sugar(&source).unwrap(), source);
    }

    #[test]
    fn a_shadowed_rc_or_ref_cell_stays_plain() {
        let source =
            "use my::Rc;\nuse std::cell::RefCell;\nfn f() { let a = Rc::new(RefCell::new(1)); }\n";
        assert_eq!(sugar(source).unwrap(), source);
        let source = with_uses("struct RefCell;\nfn f() { let a = Rc::new(RefCell::new(1)); }\n");
        assert_eq!(sugar(&source).unwrap(), source);
        let source = with_uses("fn f<Rc>() { let a = Rc::new(RefCell::new(1)); }\n");
        assert_eq!(sugar(&source).unwrap(), source);
        let source = "fn f() { let a = rc::Rc::new(cell::RefCell::new(1)); }\n";
        assert_eq!(sugar(source).unwrap(), source);
    }

    #[test]
    fn shapes_rc_cannot_write_stay_plain() {
        let source = with_uses(
            "fn f(x: u8, y: &mut u8) {\n    let a = Rc::new(RefCell::new(x)).clone();\n    \
             let b = Rc::<RefCell<u8>>::new(RefCell::new(x));\n    \
             let c = Rc::new(RefCell::<u8>::new(x));\n    \
             let d = Rc::new(RefCell::new(&x));\n    let e = Rc::new(RefCell::new(-1));\n    \
             let g = Rc::new(RefCell::new((x, x)));\n    let h = S { f: Rc::new(RefCell::new(x)) };\n    \
             let i = Rc::new(RefCell::new(x)) as Rc<RefCell<u8>>;\n    \
             let j = Rc::new(/* why */ RefCell::new(x));\n    let k = Rc::new(RefCell::new(x // why\n    ));\n    \
             let m = Rc::new(x);\n    let n = Rc::new(RefCell::new(if x > 0 { 1 } else { 2 }));\n    \
             vec![Rc::new(RefCell::new(x))];\n}\n",
        );
        assert_eq!(sugar(&source).unwrap(), source);
        // Checked without the driver, which would also drop these edits; the rule itself does
        // not match a call with a trailing comma.
        let source = source.replace("let m", "let l = Rc::new(RefCell::new(x,));\n    let m");
        let file = syn::parse_file(&source).unwrap();
        assert_eq!(RcValues.edits(&file, &source), []);
    }

    #[test]
    fn layout_inside_the_value_survives() {
        assert_sugar(
            &with_uses(
                "fn f() {\n    let a = Rc::new(RefCell::new(\n        Node {\n            // kept\n            \
                 size: 1,\n        }\n    ));\n}\n",
            ),
            &with_uses(
                "fn f() {\n    let a = rc Node {\n            // kept\n            size: 1,\n        };\n}\n",
            ),
        );
    }

    #[test]
    fn let_mut_and_rc_convert_in_one_statement() {
        assert_sugar(
            &with_uses(
                "fn f() { let mut a = Rc::new(RefCell::new(1)); a = Rc::new(RefCell::new(2)); }",
            ),
            &with_uses("fn f() { var a = rc 1; a = rc 2; }"),
        );
    }
}
