//! Accesses to a binding that must go through an opener first, such as a lock or a borrow.
//!
//! Shared by `shared` bindings (problem 12, `s.lock()`) and `rc` bindings (problem 16,
//! `borrow()` / `borrow_mut()`): they differ only in how the value is opened ([`Opening`]), so
//! the scope rules, the guard grouping and the write detection live here once.
//!
//! # Accesses and scope
//!
//! An access is a field access `s.f` / `s.0`, or a method call `s.m(..)` whose method is not
//! one of the handle's own ([`Opening::handle_methods`] and `clone`). The scope is the rest of
//! the block after the binding, nested blocks and closures included. It ends where a pattern
//! binds the name again (`let`, a closure parameter, a match arm, `if let`, `while let`, `for`),
//! except `let s = s.clone();`, which rebinds the same handle, as a capture list
//! `move[s.clone()]` expands to. Items cannot see the binding. Macro bodies made of
//! expressions (`format!`, `assert_eq!`, `vec!`) are part of the scope; other macro bodies are
//! not.
//!
//! # One guard per statement
//!
//! A guard used as a temporary (`s.lock().f`) lives until the end of its statement, and the lock
//! is not reentrant, so a second access while it lives deadlocks. Accesses are therefore grouped
//! by unit: a statement, a block tail, an `if` / `while` condition without `let` (Rust drops its
//! temporaries before the body runs) or the code of a `move` closure or an `async` block (it
//! runs apart from the statement that creates it). Within a unit, an access is direct unless it
//! sits in a nested block, a nested condition or a closure without `move`; those run while a
//! direct access's temporary still lives.
//!
//! - One direct access and nothing nested: the access opens the value itself, `s.lock().f`, so
//!   each access is a separate lock, as the problem file shows.
//! - More, or a direct access with nested ones: the unit becomes
//!   `{ let __rustx_s = &mut *s.lock(); .. }` and every access in it, nested ones included, uses
//!   that guard. A block rather than `s.with_mut(|s| ..)`: a closure would capture `return`, `?`,
//!   `break`, `continue` and `.await` of the statement, and a block keeps them working.
//! - No direct access: each nested unit is judged on its own.

use std::mem;

use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, format_ident};
use syn::ext::IdentExt;
use syn::visit_mut::{self, VisitMut};
use syn::{
    Arm, BinOp, Block, Expr, ExprAsync, ExprClosure, ExprForLoop, ExprIf, ExprMacro, ExprWhile,
    Local, Macro, Pat, PatIdent, Stmt, StmtMacro, UnOp, parse_quote, parse_quote_spanned,
};

use super::macro_args::MacroArgs;
use crate::tokens::syntax::{RESERVED_PREFIX, introduced_span};

/// `&mut self` methods of std collections, `String` and `Option`, so `s.items.push(x)` counts as
/// a write. A macro has no types, so methods are known by name only.
#[rustfmt::skip]
pub(super) const MUTATING_METHODS: &[&str] = &[
    "append", "clear", "dedup", "drain", "entry", "extend", "first_mut", "get_mut",
    "get_or_insert_with", "insert", "iter_mut", "last_mut", "next", "pop", "pop_back", "pop_front",
    "push", "push_back", "push_front", "push_str", "remove", "replace", "resize", "retain",
    "reverse", "sort", "sort_by", "sort_by_key", "sort_unstable", "swap", "take", "truncate",
    "values_mut",
];

/// How a pass opens its binding's value.
pub(super) struct Opening {
    /// The expression that opens `name`: for the one access of a unit, or, when `guard` is set,
    /// the initializer of the guard every access of the unit shares. `writes` says whether one of
    /// those accesses writes, as far as the syntax shows.
    pub(super) open: fn(name: &Ident, writes: bool, guard: bool) -> Expr,
    /// Methods of the handle itself, which stay as written.
    pub(super) handle_methods: &'static [&'static str],
}

/// How the scope uses a binding, for a pass that picks the handle type from it.
#[derive(Default)]
pub(super) struct Uses {
    /// Some access reads or writes the value.
    pub(super) accesses: bool,
    /// Some access writes: an assignment, `&mut`, or a [`MUTATING_METHODS`] call.
    pub(super) writes: bool,
    /// The handle itself is used: a handle method other than `clone`, a `clone` that is not a
    /// rebinding, or the bare name.
    pub(super) handle: bool,
}

/// Records how `scope`, the statements after the binding, uses `name`; changes nothing.
pub(super) fn scan(name: &Ident, scope: &mut [Stmt], opening: &Opening) -> Uses {
    let mut walk = Walk::new(name, opening, Mode::Scan);
    walk.stmts(scope);
    walk.uses
}

/// Makes every access to `name` in `scope` go through `opening`, grouped as the module says.
pub(super) fn rewrite(name: &Ident, scope: &mut [Stmt], opening: &Opening) {
    Walk::new(name, opening, Mode::Descend).stmts(scope);
}

enum Mode {
    Scan,
    /// Counts the accesses of the current unit.
    Count,
    /// Puts this expression in place of the binding in every access of the current unit.
    Rewrite(Box<Expr>),
    /// The current unit has no direct access: finds the units inside it.
    Descend,
}

#[derive(Default)]
struct Counts {
    depth: usize,
    direct: usize,
    nested: usize,
    writes: bool,
}

struct Walk<'a> {
    name: &'a Ident,
    opening: &'a Opening,
    mode: Mode,
    counts: Counts,
    rewrites: usize,
    uses: Uses,
}

impl<'a> Walk<'a> {
    fn new(name: &'a Ident, opening: &'a Opening, mode: Mode) -> Self {
        Self {
            name,
            opening,
            mode,
            counts: Counts::default(),
            rewrites: 0,
            uses: Uses::default(),
        }
    }

    fn is_name(&self, expr: &Expr) -> bool {
        matches!(expr, Expr::Path(p) if p.qself.is_none() && p.path.is_ident(self.name))
    }

    fn stmts(&mut self, stmts: &mut [Stmt]) {
        for stmt in stmts {
            match stmt {
                Stmt::Local(local) if self.rebinds_handle(local) => {}
                Stmt::Local(local) => {
                    if let Some(init) = &mut local.init {
                        self.part(&mut init.expr);
                        if let Some((_, diverge)) = &mut init.diverge {
                            self.part(diverge);
                        }
                    }
                    if binds(&local.pat, self.name) {
                        break;
                    }
                }
                Stmt::Expr(expr, _) => self.part(expr),
                Stmt::Macro(_) if matches!(self.mode, Mode::Descend) => self.macro_unit(stmt),
                Stmt::Macro(stmt) => self.visit_macro_mut(&mut stmt.mac),
                Stmt::Item(_) => {}
            }
        }
    }

    /// `let s = s.clone();`: the new binding is the same handle, so the scope goes on.
    fn rebinds_handle(&self, local: &Local) -> bool {
        let same_name = matches!(&local.pat, Pat::Ident(PatIdent { by_ref: None, mutability: None, subpat: None, ident, .. }) if ident == self.name);
        same_name
            && local.init.as_ref().is_some_and(|init| {
                init.diverge.is_none()
                    && matches!(&*init.expr, Expr::MethodCall(call)
                        if call.method == "clone" && call.args.is_empty() && self.is_name(&call.receiver))
            })
    }

    /// A statement's expression: a unit of its own while descending, part of the unit otherwise.
    fn part(&mut self, expr: &mut Expr) {
        if matches!(self.mode, Mode::Descend) {
            self.unit(expr);
        } else {
            self.visit_expr_mut(expr);
        }
    }

    /// A macro statement as a unit, turned into an expression statement if it gets a guard.
    fn macro_unit(&mut self, stmt: &mut Stmt) {
        let placeholder = Stmt::Expr(Expr::Verbatim(TokenStream::new()), None);
        let Stmt::Macro(StmtMacro {
            attrs,
            mac,
            semi_token,
        }) = mem::replace(stmt, placeholder)
        else {
            return;
        };
        let mut expr = Expr::Macro(ExprMacro { attrs, mac });
        self.unit(&mut expr);
        *stmt = match expr {
            Expr::Macro(ExprMacro { attrs, mac }) => Stmt::Macro(StmtMacro {
                attrs,
                mac,
                semi_token,
            }),
            other => Stmt::Expr(other, semi_token),
        };
    }

    fn unit(&mut self, expr: &mut Expr) {
        let outer = mem::replace(&mut self.mode, Mode::Count);
        let saved = mem::take(&mut self.counts);
        self.visit_expr_mut(expr);
        let counts = mem::replace(&mut self.counts, saved);
        let (open, name) = (self.opening.open, self.name);
        if counts.direct == 0 {
            self.mode = Mode::Descend;
            self.visit_expr_mut(expr);
        } else if counts.direct == 1 && counts.nested == 0 {
            self.mode = Mode::Rewrite(Box::new(open(name, counts.writes, false)));
            self.visit_expr_mut(expr);
        } else {
            let span = name.span();
            let guard = format_ident!(
                "{}{}",
                RESERVED_PREFIX,
                name.unraw(),
                span = introduced_span(span)
            );
            let init = open(name, counts.writes, true);
            self.mode = Mode::Rewrite(Box::new(parse_quote!(#guard)));
            self.visit_expr_mut(expr);
            let body = mem::replace(expr, Expr::Verbatim(TokenStream::new()));
            *expr = parse_quote_spanned!(span=> { let #guard = #init; });
            if let Expr::Block(block) = expr {
                block.block.stmts.push(Stmt::Expr(body, None));
            }
        }
        self.mode = outer;
    }

    fn access(&mut self, base: &mut Expr) {
        self.uses.accesses = true;
        match &self.mode {
            Mode::Count if self.counts.depth == 0 => self.counts.direct += 1,
            Mode::Count => self.counts.nested += 1,
            Mode::Rewrite(with) => {
                *base = Expr::clone(with);
                self.rewrites += 1;
            }
            Mode::Scan | Mode::Descend => {}
        }
    }

    fn write(&mut self) {
        self.uses.writes = true;
        self.counts.writes = true;
    }

    fn nested(&mut self, visit: impl FnOnce(&mut Self)) {
        self.counts.depth += 1;
        visit(self);
        self.counts.depth -= 1;
    }

    /// An `if` / `while` condition without `let`, a unit of its own.
    fn condition(&mut self, cond: &mut Expr) {
        if lets(cond).is_empty() {
            match self.mode {
                Mode::Descend => self.unit(cond),
                _ => self.nested(|walk| walk.visit_expr_mut(cond)),
            }
        } else {
            self.visit_expr_mut(cond);
        }
    }
}

impl VisitMut for Walk<'_> {
    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        match expr {
            Expr::Field(field) if self.is_name(&field.base) => return self.access(&mut field.base),
            Expr::MethodCall(call) if self.is_name(&call.receiver) => {
                let method = call.method.to_string();
                if method == "clone" || self.opening.handle_methods.contains(&method.as_str()) {
                    self.uses.handle = true;
                } else {
                    if MUTATING_METHODS.contains(&method.as_str()) {
                        self.write();
                    }
                    self.access(&mut call.receiver);
                }
                for arg in &mut call.args {
                    self.visit_expr_mut(arg);
                }
                return;
            }
            Expr::Path(_) if self.is_name(expr) => {
                self.uses.handle = true;
                return;
            }
            _ => {}
        }
        if writes_to(expr).is_some_and(|root| root == self.name) {
            self.write();
        }
        visit_mut::visit_expr_mut(self, expr);
    }

    fn visit_block_mut(&mut self, block: &mut Block) {
        self.nested(|walk| walk.stmts(&mut block.stmts));
    }

    fn visit_expr_closure_mut(&mut self, closure: &mut ExprClosure) {
        if closure.inputs.iter().any(|pat| binds(pat, self.name)) {
            return;
        }
        let body = &mut *closure.body;
        match self.mode {
            Mode::Count if closure.capture.is_some() => {}
            Mode::Scan | Mode::Count => self.nested(|walk| walk.visit_expr_mut(body)),
            Mode::Rewrite(_) if closure.capture.is_none() => self.visit_expr_mut(body),
            Mode::Rewrite(_) | Mode::Descend => self.unit(body),
        }
    }

    fn visit_expr_async_mut(&mut self, block: &mut ExprAsync) {
        match self.mode {
            Mode::Count => {}
            Mode::Scan => self.stmts(&mut block.block.stmts),
            Mode::Rewrite(_) | Mode::Descend => {
                let outer = mem::replace(&mut self.mode, Mode::Descend);
                self.stmts(&mut block.block.stmts);
                self.mode = outer;
            }
        }
    }

    fn visit_expr_if_mut(&mut self, node: &mut ExprIf) {
        self.condition(&mut node.cond);
        if !lets(&node.cond).iter().any(|pat| binds(pat, self.name)) {
            self.visit_block_mut(&mut node.then_branch);
        }
        if let Some((_, else_branch)) = &mut node.else_branch {
            self.visit_expr_mut(else_branch);
        }
    }

    fn visit_expr_while_mut(&mut self, node: &mut ExprWhile) {
        self.condition(&mut node.cond);
        if !lets(&node.cond).iter().any(|pat| binds(pat, self.name)) {
            self.visit_block_mut(&mut node.body);
        }
    }

    fn visit_expr_for_loop_mut(&mut self, node: &mut ExprForLoop) {
        self.visit_expr_mut(&mut node.expr);
        if !binds(&node.pat, self.name) {
            self.visit_block_mut(&mut node.body);
        }
    }

    fn visit_arm_mut(&mut self, arm: &mut Arm) {
        if !binds(&arm.pat, self.name) {
            visit_mut::visit_arm_mut(self, arm);
        }
    }

    fn visit_macro_mut(&mut self, mac: &mut Macro) {
        let Ok(mut args) = mac.parse_body::<MacroArgs>() else {
            return;
        };
        let before = self.rewrites;
        for expr in args.exprs_mut() {
            self.visit_expr_mut(expr);
        }
        if self.rewrites != before {
            mac.tokens = args.into_token_stream();
        }
    }
}

/// The binding an expression writes to: the place an assignment or `&mut` takes, or the
/// receiver of a [`MUTATING_METHODS`] call, traced back to its root name.
fn writes_to(expr: &Expr) -> Option<&Ident> {
    let place = match expr {
        Expr::Assign(assign) => &assign.left,
        Expr::Binary(binary) if is_compound_assign(&binary.op) => &binary.left,
        Expr::Reference(reference) if reference.mutability.is_some() => &reference.expr,
        Expr::MethodCall(call) if MUTATING_METHODS.contains(&call.method.to_string().as_str()) => {
            &call.receiver
        }
        _ => return None,
    };
    root(place)
}

fn root(place: &Expr) -> Option<&Ident> {
    match place {
        Expr::Path(path) if path.qself.is_none() => path.path.get_ident(),
        Expr::Field(field) => root(&field.base),
        Expr::Index(index) => root(&index.expr),
        Expr::Paren(paren) => root(&paren.expr),
        Expr::Unary(unary) if matches!(unary.op, UnOp::Deref(_)) => root(&unary.expr),
        _ => None,
    }
}

fn is_compound_assign(op: &BinOp) -> bool {
    matches!(
        op,
        BinOp::AddAssign(_)
            | BinOp::SubAssign(_)
            | BinOp::MulAssign(_)
            | BinOp::DivAssign(_)
            | BinOp::RemAssign(_)
            | BinOp::BitXorAssign(_)
            | BinOp::BitAndAssign(_)
            | BinOp::BitOrAssign(_)
            | BinOp::ShlAssign(_)
            | BinOp::ShrAssign(_)
    )
}

/// The patterns of the `let`s in an `if` / `while` condition, let chains included.
fn lets(cond: &Expr) -> Vec<&Pat> {
    match cond {
        Expr::Let(binding) => vec![&*binding.pat],
        Expr::Binary(binary) if matches!(binary.op, BinOp::And(_)) => {
            let mut pats = lets(&binary.left);
            pats.extend(lets(&binary.right));
            pats
        }
        _ => Vec::new(),
    }
}

/// Whether `pat` binds `name`, which shadows the tracked binding.
fn binds(pat: &Pat, name: &Ident) -> bool {
    struct Finds<'a>(&'a Ident, bool);
    impl VisitMut for Finds<'_> {
        fn visit_pat_ident_mut(&mut self, pat: &mut PatIdent) {
            self.1 |= pat.ident == *self.0;
            visit_mut::visit_pat_ident_mut(self, pat);
        }
        // Guards and range bounds are expressions, which bind nothing.
        fn visit_expr_mut(&mut self, _: &mut Expr) {}
    }
    let mut finds = Finds(name, false);
    finds.visit_pat_mut(&mut pat.clone());
    finds.1
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;
    use syn::parse::Parser;

    /// An opening that shows what each unit tells it, as `rc` bindings need for choosing
    /// between `borrow()` and `borrow_mut()`.
    const PROBE: Opening = Opening {
        open: probe,
        handle_methods: &["borrow"],
    };

    fn probe(name: &Ident, writes: bool, guard: bool) -> Expr {
        parse_quote!(#name.open(#writes, #guard))
    }

    fn block(body: &TokenStream) -> Block {
        parse_quote!({ #body })
    }

    #[test]
    fn the_opening_learns_whether_the_accesses_it_serves_write() {
        let mut body = block(&quote! {
            s.a += 1;
            let x = s.b;
            s.v.push(x);
            s.borrow();
            (s.a, s.b)
        });
        rewrite(&format_ident!("s"), &mut body.stmts, &PROBE);
        let want = block(&quote! {
            s.open(true, false).a += 1;
            let x = s.open(false, false).b;
            s.open(true, false).v.push(x);
            s.borrow();
            { let __rustx_s = s.open(false, true); (__rustx_s.a, __rustx_s.b) }
        });
        assert_eq!(
            body.into_token_stream().to_string(),
            want.into_token_stream().to_string()
        );
    }

    #[test]
    fn scan_tells_reads_writes_and_handle_uses_apart() {
        let uses = |body: TokenStream| {
            let found = scan(&format_ident!("s"), &mut block(&body).stmts, &PROBE);
            (found.accesses, found.writes, found.handle)
        };
        assert_eq!(uses(quote!(s.a; s.len();)), (true, false, false));
        assert_eq!(uses(quote!(s.a.insert(1);)), (true, true, false));
        assert_eq!(uses(quote!(s.borrow(); f(s);)), (false, false, true));
        assert_eq!(uses(quote!(let s = s.clone(); s.a;)), (true, false, false));
        assert_eq!(uses(quote!(let t = s.clone();)), (false, false, true));
        assert_eq!(uses(quote!(let s = 1; s.a = 2;)), (false, false, false));
    }

    #[test]
    fn writes_are_traced_to_the_root_of_the_place() {
        for (source, root) in [
            ("s.a = 1", Some("s")),
            ("s.a[0].1 += 1", Some("s")),
            ("&mut (*s).a", Some("s")),
            ("s.items.push(1)", Some("s")),
            ("f(s).a = 1", None),
            ("s.a.len()", None),
            ("&s.a", None),
        ] {
            let expr: Expr = syn::parse_str(source).unwrap();
            let found = writes_to(&expr).map(ToString::to_string);
            assert_eq!(found.as_deref(), root, "{source}");
        }
    }

    #[test]
    fn a_pattern_binds_every_name_inside_it() {
        let name = format_ident!("s");
        for (source, bound) in [
            ("s", true),
            ("Some((a, s))", true),
            ("S { s, .. }", true),
            ("x @ Some(s)", true),
            ("Some(x)", false),
            ("S { s: x }", false),
        ] {
            let pat = Pat::parse_single.parse_str(source).unwrap();
            assert_eq!(binds(&pat, &name), bound, "{source}");
        }
    }
}
