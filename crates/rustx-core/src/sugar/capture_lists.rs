//! `{ let a = a.clone(); move || body }` back to `move[a.clone()] || body`: the reverse of
//! `passes/capture_lists.rs` (problem 11, T19.14).
//!
//! Only a block expression made of `let name = capture;` statements and a `move` closure as its
//! tail is matched, since that is the block the pass builds. Whether the captures name their
//! bindings the way the pass does (`a.clone()` binds `a`, no name twice, no `self`) is asked of the
//! pass itself rather than of a copy of its rules: the sugar is built from the block and must read
//! back as the same tokens. So a `let` with a type, `mut`, a pattern or `else`, other statements, a
//! non-`move`, `static` or `for<..>` closure and a renamed binding all stay as written. A fn body
//! is not a block expression, so a body made of such statements stays too: the pass would wrap it
//! in a second block.
//!
//! The edits replace only the bytes between the captures and around the closure, so comments and
//! layout inside a capture or the closure survive and other rules' edits there do not overlap.
//! A comment anywhere else in the block, which those edits would drop, keeps the block as written.
//! Where the block sits decides where the sugar's closure ends (`{ .. }()` would call the body);
//! the driver's round trip drops an edit read that way. A match arm whose block body has no `,`
//! and another arm after it stays too: the sugar would run into the next arm with nothing between
//! them, and a `,` added there is a token the expansion does not write, so the check refuses it.
//! Blocks inside a macro call's arguments stay, as for `var`: `syn` does not parse macro bodies.

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::visit::{self, Visit};
use syn::{Expr, ExprBlock, ExprClosure, ExprMatch, Pat, Stmt};

use super::{Edit, ReverseRule, has_comment, span_range};

pub(crate) struct CaptureLists;

impl ReverseRule for CaptureLists {
    fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit> {
        self.edit_groups(file, source).concat()
    }

    /// One set per block: removing its `{` without its `}` would not even parse.
    fn edit_groups(&self, file: &syn::File, source: &str) -> Vec<Vec<Edit>> {
        let mut finder = Finder {
            source,
            unseparated: Vec::new(),
            groups: Vec::new(),
        };
        finder.visit_file(file);
        finder.groups
    }
}

struct Finder<'a> {
    source: &'a str,
    /// Where the arm bodies that end without a `,` before the next arm start.
    unseparated: Vec<usize>,
    groups: Vec<Vec<Edit>>,
}

impl<'ast> Visit<'ast> for Finder<'_> {
    fn visit_expr_match(&mut self, expr: &'ast ExprMatch) {
        let followed = expr.arms.split_last().map_or(&[][..], |(_, rest)| rest);
        for arm in followed.iter().filter(|arm| arm.comma.is_none()) {
            if let Expr::Block(block) = &*arm.body {
                let open = block.block.brace_token.span.open();
                self.unseparated.push(open.byte_range().start);
            }
        }
        visit::visit_expr_match(self, expr);
    }

    fn visit_expr_block(&mut self, block: &'ast ExprBlock) {
        let open = block.block.brace_token.span.open().byte_range().start;
        if !self.unseparated.contains(&open) {
            self.groups.extend(capture_list(self.source, block));
        }
        visit::visit_expr_block(self, block);
    }
}

/// The edits turning `block` into a capture list, when the pass would build it from one.
fn capture_list(source: &str, block: &ExprBlock) -> Option<Vec<Edit>> {
    if !block.attrs.is_empty() || block.label.is_some() {
        return None;
    }
    let (tail, lets) = block.block.stmts.split_last()?;
    let Stmt::Expr(Expr::Closure(closure), None) = tail else {
        return None;
    };
    let keyword = closure.capture?.span;
    let captures = lets.iter().map(capture).collect::<Option<Vec<_>>>()?;
    if !reads_back(block, &captures, closure) {
        return None;
    }

    let ranges = captures
        .iter()
        .map(|capture| span_range(capture.to_token_stream()))
        .collect::<Option<Vec<_>>>()?;
    let closure_end = span_range(closure.to_token_stream())?.end;
    let (open, close) = (
        block.block.brace_token.span.open().byte_range(),
        block.block.brace_token.span.close().byte_range(),
    );
    let prefix = match closure.asyncness {
        Some(_) => "async move[",
        None => "move[",
    };
    // From `{` to the first capture, between captures, from the last capture to `move` and from
    // the closure to `}` there are only `let`, names, `=`, `;`, `async` and `move`.
    let mut edits = vec![Edit {
        range: open.start..ranges[0].start,
        text: prefix.to_owned(),
    }];
    for pair in ranges.windows(2) {
        edits.push(Edit {
            range: pair[0].end..pair[1].start,
            text: ", ".to_owned(),
        });
    }
    edits.push(Edit {
        range: ranges[ranges.len() - 1].end..keyword.byte_range().end,
        text: "]".to_owned(),
    });
    edits.push(Edit {
        range: closure_end..close.end,
        text: String::new(),
    });
    // A comment in those bytes would be dropped with them.
    let bare = edits.iter().all(|edit| {
        source
            .get(edit.range.clone())
            .is_some_and(|gap| !has_comment(gap))
    });
    bare.then_some(edits)
}

/// The value of `let name = value;`, the only statement the pass writes before the closure.
fn capture(stmt: &Stmt) -> Option<&Expr> {
    let Stmt::Local(local) = stmt else {
        return None;
    };
    let Pat::Ident(_) = &local.pat else {
        return None;
    };
    let init = local.init.as_ref()?;
    init.diverge.is_none().then_some(&*init.expr)
}

/// Whether the token passes read `[async] move[captures] closure` back as `block`. The closure is
/// written without `async move`, which the sugar puts around the list.
fn reads_back(block: &ExprBlock, captures: &[&Expr], closure: &ExprClosure) -> bool {
    let mut rest = closure.clone();
    let asyncness = rest.asyncness.take();
    rest.capture = None;
    let sugar: TokenStream = quote!(#asyncness move [#(#captures),*] #rest);
    let plain = block.to_token_stream().to_string();
    crate::passes::rewrite_tokens(sugar).is_ok_and(|tokens| tokens.to_string() == plain)
}

#[cfg(test)]
mod tests {
    use super::CaptureLists;
    use crate::sugar::tests::assert_sugar;
    use crate::sugar::{ReverseRule, sugar};

    /// The source comes back unchanged, and not because the driver dropped an edit.
    fn assert_no_edits(source: &str) {
        let file = syn::parse_file(source).unwrap();
        assert_eq!(CaptureLists.edits(&file, source), []);
        assert_eq!(sugar(source).unwrap(), source);
    }

    #[test]
    fn let_bindings_before_a_move_closure_become_a_capture_list() {
        assert_sugar(
            "fn f() { spawn({ let a = a.clone(); let b = b.clone(); move || work(a, b) }); }",
            "fn f() { spawn(move[a.clone(), b.clone()] || work(a, b)); }",
        );
    }

    #[test]
    fn a_capture_binds_the_variable_its_method_chain_starts_with() {
        assert_sugar(
            "fn f() { let g = { let rx = rx.lock().unwrap().clone(); let n = n; \
             let v = v.iter::<u8>(); move |x: u8| x + n }; }",
            "fn f() { let g = move[rx.lock().unwrap().clone(), n, v.iter<u8>()] |x: u8| x + n; }",
        );
    }

    #[test]
    fn an_async_closure_keeps_async_in_front_of_move() {
        assert_sugar(
            "fn f() { run({ let a = a.clone(); async move |x| a.get(x).await }); }",
            "fn f() { run(async move[a.clone()] |x| a.get(x).await); }",
        );
    }

    #[test]
    fn a_block_over_lines_with_a_return_type_converts_and_its_layout_inside_survives() {
        assert_sugar(
            "fn counter(name: Arc<String>) -> impl Fn() -> usize {\n    {\n        \
             let name = name.clone();\n        move || -> usize {\n            \
             // counted\n            name.len()\n        }\n    }\n}\n",
            "fn counter(name: Arc<String>) -> impl Fn() -> usize {\n    \
             move[name.clone()] || -> usize {\n            \
             // counted\n            name.len()\n        }\n}\n",
        );
    }

    #[test]
    fn nested_blocks_each_become_a_capture_list() {
        assert_sugar(
            "fn f() { let g = { let a = a.clone(); move || { let b = b.clone(); move || a + b } }; \
             f(|| { g({ let c = c; move || c }) }); }",
            "fn f() { let g = move[a.clone()] || move[b.clone()] || a + b; \
             f(|| { g(move[c] || c) }); }",
        );
    }

    #[test]
    fn var_inside_the_closure_and_a_capture_list_convert_together() {
        assert_sugar(
            "fn f() { let g = { let a = a.clone(); move || { let mut n = a; n += 1; n } }; }",
            "fn f() { let g = move[a.clone()] || { var n = a; n += 1; n }; }",
        );
    }

    #[test]
    fn blocks_the_pass_does_not_build_stay_byte_for_byte() {
        assert_no_edits(
            "fn f() {\n    let g = { let b = a.clone(); move || b };\n    \
             let g = { let a = a.clone(); || a };\n    \
             let g = { let a: Arc<u8> = a.clone(); move || a };\n    \
             let g = { let (a, b) = (a, b); move || a };\n    \
             let g = { let a = a.clone(); log(); move || a };\n    \
             let g = { let a = a.clone(); move || a; };\n    \
             let g = { let a = a.clone(); let a = a.lock(); move || a };\n    \
             let g = { let cfg = self.cfg.clone(); move || cfg };\n    \
             let g = { let a = Arc::clone(&a); move || a };\n    \
             let g = { let s = self; move || s };\n    \
             let g = { let Some(a) = a else { return }; move || a };\n    \
             let g = { #[allow(unused)] let a = a; move || a };\n    \
             let g = { let a = a; for<'b> move |x: &'b u8| a };\n    \
             let g = { move || a };\n    \
             let g = unsafe { let a = a; move || a };\n    \
             let g = 'l: { let a = a; move || a };\n    \
             let g = { let a = a; async move { a } };\n    \
             let g = vec![{ let a = a; move || a }];\n}\n",
        );
    }

    #[test]
    fn a_mutable_binding_becomes_var_but_keeps_its_block() {
        assert_sugar(
            "fn f() { let g = { let mut a = a.clone(); move || a }; }",
            "fn f() { let g = { var a = a.clone(); move || a }; }",
        );
    }

    #[test]
    fn a_match_arm_body_converts_where_a_comma_or_the_match_end_follows_it() {
        assert_sugar(
            "fn f(x: Option<u8>) {\n    let g = match x {\n        Some(_) => {\n            \
             let a = a.clone();\n            move || *a\n        },\n        \
             None => { let b = b.clone(); move || *b }\n    };\n}\n",
            "fn f(x: Option<u8>) {\n    let g = match x {\n        \
             Some(_) => move[a.clone()] || *a,\n        \
             None => move[b.clone()] || *b\n    };\n}\n",
        );
    }

    #[test]
    fn a_match_arm_body_without_a_comma_before_the_next_arm_stays() {
        assert_no_edits(
            "fn f(x: Option<u8>) {\n    let g = match x {\n        Some(_) => {\n            \
             let a = a.clone();\n            move || *a\n        }\n        \
             None => unreachable!(),\n    };\n}\n",
        );
    }

    #[test]
    fn a_block_in_statement_position_converts_with_its_semicolon_or_as_a_tail() {
        assert_sugar(
            "fn f() { { let a = a.clone(); move || a }; }\n\
             fn g() -> impl Fn() -> u8 { let n = 1; { let a = a; move || a + n } }\n",
            "fn f() { move[a.clone()] || a; }\n\
             fn g() -> impl Fn() -> u8 { let n = 1; move[a] || a + n }\n",
        );
    }

    #[test]
    fn a_fn_body_made_of_the_bindings_is_not_a_block_the_pass_builds() {
        assert_no_edits("fn f() -> impl Fn() -> u8 { let a = a.clone(); move || a }\n");
    }

    #[test]
    fn a_comment_between_the_bindings_keeps_the_block() {
        assert_no_edits(
            "fn f() {\n    let g = {\n        // why\n        let a = a.clone();\n        \
             move || a\n    };\n    let h = { let a = /* why */ a; move || a };\n    \
             let k = { let a = a; move || a /* why */ };\n}\n",
        );
    }

    #[test]
    fn a_comment_inside_a_capture_survives() {
        assert_sugar(
            "fn f() { let g = { let a = a /* shared */ .clone(); move || a }; }",
            "fn f() { let g = move[a /* shared */ .clone()] || a; }",
        );
    }

    #[test]
    fn a_block_whose_closure_would_take_what_follows_it_stays() {
        // The rule proposes it; the driver's round trip reads `move[a] || a()` and drops it.
        let source = "fn f() { let n = { let a = a; move || a }(); }\n";
        assert_eq!(sugar(source).unwrap(), source);
    }
}
