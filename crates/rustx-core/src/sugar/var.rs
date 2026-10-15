//! `let mut x = ..` back to `var x = ..`: the reverse of `passes/var.rs` (problem 02).
//!
//! Only the shapes the `var` pass emits are matched: a `let` statement whose pattern is one
//! mutable by-value name, optionally `name @ subpattern` and `: T`, with or without `= e` and
//! `else { .. }`. Destructuring (`let (mut a, b)`), `let ref mut` and `let mut` in `if let`,
//! `while let` or a `match` arm stay, since `var` cannot write them. `let mut` inside a macro
//! call's arguments stays as well: `syn` does not parse macro bodies, so nothing proves the
//! tokens there are a statement.

use syn::visit::{self, Visit};
use syn::{Local, Pat, PatIdent};

use super::{Edit, ReverseRule};

pub(crate) struct LetMut;

impl ReverseRule for LetMut {
    fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit> {
        let mut finder = Finder {
            source,
            edits: Vec::new(),
        };
        finder.visit_file(file);
        finder.edits
    }
}

struct Finder<'a> {
    source: &'a str,
    edits: Vec<Edit>,
}

impl<'ast> Visit<'ast> for Finder<'_> {
    fn visit_local(&mut self, local: &'ast Local) {
        let pat = match &local.pat {
            Pat::Type(typed) => &*typed.pat,
            pat => pat,
        };
        if let Pat::Ident(PatIdent {
            by_ref: None,
            mutability: Some(mutability),
            ..
        }) = pat
        {
            let edit = Edit::between(local.let_token.span, mutability.span, "var");
            // Replacing `let /* why */ mut` as a whole would delete the comment.
            let gap = edit.range.start + "let".len()..edit.range.end - "mut".len();
            if self
                .source
                .get(gap)
                .is_some_and(|gap| gap.chars().all(char::is_whitespace))
            {
                self.edits.push(edit);
            }
        }
        visit::visit_local(self, local);
    }
}

#[cfg(test)]
mod tests {
    use crate::sugar::sugar;
    use crate::sugar::tests::assert_sugar;

    #[test]
    fn let_mut_with_a_value_a_type_or_neither_becomes_var() {
        assert_sugar(
            "fn f() { let mut x = 1; let mut y: u8 = 2; let mut z; z = 3; }",
            "fn f() { var x = 1; var y: u8 = 2; var z; z = 3; }",
        );
    }

    #[test]
    fn subpattern_let_else_attributes_and_raw_names_keep_their_text() {
        assert_sugar(
            "fn f(d: u8) {\n    #[allow(unused)]\n    let mut n @ 1..=9 = d else { return };\n    \
             let mut r#var = 0;\n}\n",
            "fn f(d: u8) {\n    #[allow(unused)]\n    var n @ 1..=9 = d else { return };\n    \
             var r#var = 0;\n}\n",
        );
    }

    #[test]
    fn let_mut_in_nested_fns_closures_blocks_and_impls_is_found() {
        assert_sugar(
            "impl S { fn m(&self) { let g = || { let mut a = 1; }; if c { let mut b = 2; } \
             async { let mut c = 3; }; fn inner() { let mut d = 4; } } }",
            "impl S { fn m(&self) { let g = || { var a = 1; }; if c { var b = 2; } \
             async { var c = 3; }; fn inner() { var d = 4; } } }",
        );
    }

    #[test]
    fn comments_and_layout_around_the_statement_survive() {
        assert_sugar(
            "/// Doc.\nfn f() {\n    // count\n    let  mut\n\tx = 1; // trailing\n    x += 1;\n}\n",
            "/// Doc.\nfn f() {\n    // count\n    var\n\tx = 1; // trailing\n    x += 1;\n}\n",
        );
    }

    #[test]
    fn a_comment_between_let_and_mut_keeps_the_statement() {
        let source = "fn f() { let /* why */ mut x = 1; x += 1; }";
        assert_sugar(source, source);
    }

    #[test]
    fn shapes_var_cannot_write_stay_byte_for_byte() {
        let source = "fn f(o: Option<u8>, p: (u8, u8)) {\n    let (mut a, b) = p;\n    \
                      let ref mut r = 1;\n    let x = 2;\n    if let Some(mut v) = o { v += 1; }\n    \
                      while let Some(mut w) = o { w += 1; }\n    match o { Some(mut m) => m += 1, None => {} }\n    \
                      let Some(mut s) = o else { return };\n    for mut i in 0..a { i += b; }\n    \
                      let _ = |mut c: u8| c += 1;\n    vec![{ let mut q = 1; q }];\n}\n";
        assert_eq!(sugar(source).unwrap(), source);
    }
}
