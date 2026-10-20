//! Explicit borrows on an `rc` binding, removed together with the binding's conversion: the
//! reverse of `passes/rc_borrows.rs` (problem 16, T19.15).
//!
//! Once `rc_values` or `prefix_types` turns a binding's value or type into `rc`, `rc_borrows`
//! borrows on every field access and value-method call of the name. Plain code that already
//! borrows there (`n.borrow().size`, `n.borrow_mut().push(x)`) then reads as sugar without those
//! borrows, and this module finds them. It keeps their removal only when the pass adds back exactly
//! those borrows to the binding's scope, and only all of them or none: a binding with any other
//! borrow of the name (a held guard `let g = n.borrow();`, `*n.borrow_mut() += 1`, `try_borrow`,
//! a borrow passed to a call or written as `RefCell::borrow(&n)`), with a borrow the pass would
//! write differently (two accesses in one statement share one guard, a write through a method the
//! pass does not know, `n.borrow().clone()`, which the pass reads as the handle's `clone`) or with
//! a comment inside a borrow call keeps every borrow as written. The binding converts either way;
//! the rules put the removals in the same set as the binding's own edit, so the driver keeps or
//! drops them together.
//!
//! The borrows are found on tokens, so the ones inside macro bodies the pass reads (`format!`,
//! `assert_eq!`) are found too; the check against the pass decides which removals are right.

use proc_macro2::{Delimiter, Ident, TokenStream, TokenTree};
use quote::quote;
use syn::{Block, Stmt};

use super::Edit;
use crate::tokens::map_sequences;
use crate::tokens::punct::{is_alone, is_punct};

/// The borrows the pass writes on an access, so the ones removed here.
const ACCESS_BORROWS: &[&str] = &["borrow", "borrow_mut"];

/// Every way to borrow a `RefCell`: one left in the scope keeps the explicit borrows.
const ALL_BORROWS: &[&str] = &["borrow", "borrow_mut", "try_borrow", "try_borrow_mut"];

/// Edits removing every explicit `.borrow()` / `.borrow_mut()` that `rc_borrows` adds back to
/// `scope` once `name` is an `rc` binding, or none when that is not all of the name's borrows.
pub(super) fn removals(name: &Ident, scope: &[Stmt], source: &str) -> Vec<Edit> {
    let written = quote!(#(#scope)*);
    let mut edits = Vec::new();
    let Ok(stripped) = map_sequences(written.clone(), &mut |seq| {
        Ok(strip(name, &seq, &mut edits))
    }) else {
        return Vec::new();
    };
    let clean = edits.iter().all(|edit| {
        source.get(edit.range.clone()).is_some_and(|text| {
            let text: String = text.split_whitespace().collect();
            ACCESS_BORROWS
                .iter()
                .any(|method| text == format!(".{method}()"))
        })
    });
    if edits.is_empty() || !clean || borrows(name, stripped.clone()) {
        return Vec::new();
    }
    let Ok(block) = syn::parse2::<Block>(quote!({ #stripped })) else {
        return Vec::new();
    };
    let mut stmts = block.stmts;
    crate::passes::add_borrows(name, &mut stmts);
    if quote!(#(#stmts)*).to_string() == written.to_string() {
        edits
    } else {
        Vec::new()
    }
}

/// `seq` without the `.borrow()` / `.borrow_mut()` between `name` and an access, recording an
/// edit for each.
fn strip(name: &Ident, seq: &[TokenTree], edits: &mut Vec<Edit>) -> Vec<TokenTree> {
    let mut out = Vec::with_capacity(seq.len());
    let mut i = 0;
    while i < seq.len() {
        if let Some(edit) = access_borrow(name, seq, i) {
            edits.push(edit);
            out.push(seq[i].clone());
            i += 4;
        } else {
            out.push(seq[i].clone());
            i += 1;
        }
    }
    out
}

/// The edit removing `.borrow()` when `seq[at..]` is `name.borrow().` followed by a field or
/// method, as the pass writes an access.
fn access_borrow(name: &Ident, seq: &[TokenTree], at: usize) -> Option<Edit> {
    let [
        dot,
        TokenTree::Ident(method),
        TokenTree::Group(args),
        next,
        after,
    ] = seq.get(at + 1..at + 6)?
    else {
        return None;
    };
    let plain = is_binding(name, seq, at)
        && is_alone(dot, '.')
        && ACCESS_BORROWS.iter().any(|borrow| method == borrow)
        && args.delimiter() == Delimiter::Parenthesis
        && args.stream().is_empty()
        && is_alone(next, '.')
        && matches!(after, TokenTree::Ident(_) | TokenTree::Literal(_));
    plain.then(|| Edit::between(dot.span(), args.span_close(), ""))
}

/// Whether `seq[at]` is `name` itself rather than a field or path segment of that name.
fn is_binding(name: &Ident, seq: &[TokenTree], at: usize) -> bool {
    let TokenTree::Ident(ident) = &seq[at] else {
        return false;
    };
    let before = at.checked_sub(1).map(|i| &seq[i]);
    // A `.` before it is a field `x.n` unless it ends a range `0..n`.
    let field = before.is_some_and(|tree| is_alone(tree, '.'))
        && !at.checked_sub(2).is_some_and(|i| is_punct(&seq[i], '.'));
    ident == name && !field && !before.is_some_and(|tree| is_punct(tree, ':'))
}

/// Whether `tokens` still borrow `name`: `name.borrow()` and the like, or a path to one of those
/// methods called on `&name`.
fn borrows(name: &Ident, tokens: TokenStream) -> bool {
    let mut found = false;
    let _ = map_sequences(tokens, &mut |seq| {
        found |= (0..seq.len()).any(|i| {
            let method = |tree: Option<&TokenTree>| {
                matches!(tree, Some(TokenTree::Ident(m)) if ALL_BORROWS.iter().any(|b| m == b))
            };
            let call = is_binding(name, &seq, i)
                && seq.get(i + 1).is_some_and(|dot| is_alone(dot, '.'))
                && method(seq.get(i + 2));
            let path_call = method(seq.get(i))
                && i.checked_sub(1).is_some_and(|c| is_punct(&seq[c], ':'))
                && matches!(seq.get(i + 1), Some(TokenTree::Group(args))
                    if args.stream().into_iter().any(|tree| matches!(&tree, TokenTree::Ident(n) if n == name)));
            call || path_call
        });
        Ok(seq)
    });
    found
}

#[cfg(test)]
mod tests {
    use crate::sugar::prefix_types::PrefixTypes;
    use crate::sugar::rc_values::RcValues;
    use crate::sugar::tests::assert_sugar;
    use crate::sugar::{Edit, ReverseRule, convert, sugar};

    const USES: &str = "use std::cell::RefCell;\nuse std::rc::Rc;\n";

    fn with_uses(body: &str) -> String {
        format!("{USES}{body}")
    }

    /// Asserts the binding converts but its body keeps every borrow as written.
    fn keeps_borrows(plain_fn: &str, converted_fn: &str) {
        assert_sugar(&with_uses(plain_fn), &with_uses(converted_fn));
    }

    #[test]
    fn borrows_the_pass_adds_back_are_removed_with_the_type() {
        assert_sugar(
            &with_uses(
                "fn grow(n: &Rc<RefCell<Node>>, extra: u64) -> usize {\n    \
                 n.borrow_mut().size += extra;\n    n.borrow_mut().children.push(leaf());\n    \
                 let tag = n.borrow().name.clone();\n    \
                 println!(\"{tag}: {}\", n.borrow().size);\n    n.borrow().children.len()\n}\n",
            ),
            &with_uses(
                "fn grow(n: &rc Node, extra: u64) -> usize {\n    \
                 n.size += extra;\n    n.children.push(leaf());\n    \
                 let tag = n.name.clone();\n    \
                 println!(\"{tag}: {}\", n.size);\n    n.children.len()\n}\n",
            ),
        );
    }

    #[test]
    fn borrows_on_an_rc_value_let_are_removed_with_the_value() {
        assert_sugar(
            &with_uses(
                "fn f() -> Vec<u8> {\n    let list = Rc::new(RefCell::new(Vec::new()));\n    \
                 list.borrow_mut().push(1);\n    let len = list.borrow().len();\n    \
                 list.borrow().clone_items(len)\n}\n",
            ),
            &with_uses(
                "fn f() -> Vec<u8> {\n    let list = rc Vec::new();\n    \
                 list.push(1);\n    let len = list.len();\n    \
                 list.clone_items(len)\n}\n",
            ),
        );
    }

    #[test]
    fn typed_lets_closure_parameters_and_tuple_fields_lose_their_borrows() {
        assert_sugar(
            &with_uses(
                "fn f(v: &[Rc<RefCell<Pair>>]) -> u8 {\n    \
                 let first: Rc<RefCell<Pair>> = v[0].clone();\n    \
                 let sum: u8 = v.iter().map(|p: &Rc<RefCell<Pair>>| p.borrow().0).sum();\n    \
                 first.borrow().1 + sum\n}\n",
            ),
            &with_uses(
                "fn f(v: &[Rc<RefCell<Pair>>]) -> u8 {\n    \
                 let first: rc Pair = v[0].clone();\n    \
                 let sum: u8 = v.iter().map(|p: &rc Pair| p.0).sum();\n    \
                 first.1 + sum\n}\n",
            ),
        );
    }

    #[test]
    fn comments_and_layout_around_a_removed_borrow_survive() {
        assert_sugar(
            &with_uses(
                "fn f(n: Rc<RefCell<Node>>) -> u64 {\n    n.borrow()\n        // the size\n        \
                 .size\n}\n",
            ),
            &with_uses("fn f(n: rc Node) -> u64 {\n    n\n        // the size\n        .size\n}\n"),
        );
    }

    #[test]
    fn a_held_guard_keeps_every_borrow() {
        keeps_borrows(
            "fn f(n: Rc<RefCell<Node>>) -> u64 {\n    let g = n.borrow();\n    \
             n.borrow().size + g.size\n}\n",
            "fn f(n: rc Node) -> u64 {\n    let g = n.borrow();\n    \
             n.borrow().size + g.size\n}\n",
        );
    }

    #[test]
    fn a_deref_try_borrow_or_borrow_passed_on_keeps_every_borrow() {
        for other in [
            "*n.borrow_mut() = Node::default();",
            "let ok = n.try_borrow().is_ok();",
            "let ok = n.try_borrow_mut().is_ok();",
            "show(n.borrow());",
            "show(RefCell::borrow(&n));",
        ] {
            keeps_borrows(
                &format!(
                    "fn f(n: Rc<RefCell<Node>>) -> u64 {{\n    {other}\n    n.borrow().size\n}}\n"
                ),
                &format!("fn f(n: rc Node) -> u64 {{\n    {other}\n    n.borrow().size\n}}\n"),
            );
        }
    }

    #[test]
    fn borrows_the_pass_would_write_differently_are_kept() {
        for (plain, sugar) in [
            // Two accesses in one statement share one guard.
            (
                "n.borrow().size + n.borrow().size",
                "n.borrow().size + n.borrow().size",
            ),
            // A write through a method the pass does not know as one.
            ("n.borrow_mut().grow()", "n.borrow_mut().grow()"),
            // A read the pass would make a write.
            (
                "n.borrow().size == { n.borrow_mut().size = 1; 1 }",
                "n.borrow().size == { n.borrow_mut().size = 1; 1 }",
            ),
            // `clone` on the binding is the handle's own.
            ("n.borrow().clone().size", "n.borrow().clone().size"),
            // A macro body that is not a list of expressions is not the pass's to read.
            (
                "m!(n.borrow().size => 1);\n    n.borrow().size",
                "m!(n.borrow().size => 1);\n    n.borrow().size",
            ),
        ] {
            keeps_borrows(
                &format!("fn f(n: Rc<RefCell<Node>>) -> u64 {{\n    {plain}\n}}\n"),
                &format!("fn f(n: rc Node) -> u64 {{\n    {sugar}\n}}\n"),
            );
        }
    }

    #[test]
    fn a_comment_inside_a_borrow_call_keeps_every_borrow() {
        keeps_borrows(
            "fn f(n: Rc<RefCell<Node>>) -> u64 {\n    n.borrow(/* shared */).size\n}\n",
            "fn f(n: rc Node) -> u64 {\n    n.borrow(/* shared */).size\n}\n",
        );
    }

    #[test]
    fn a_field_or_path_of_the_same_name_keeps_its_borrow() {
        keeps_borrows(
            "fn f(n: Rc<RefCell<Node>>, s: S) -> u64 {\n    \
             let a = s.n.borrow().size + m::n.borrow().size;\n    a + n.borrow().size\n}\n",
            "fn f(n: rc Node, s: S) -> u64 {\n    \
             let a = s.n.borrow().size + m::n.borrow().size;\n    a + n.size\n}\n",
        );
    }

    #[test]
    fn a_binding_the_pass_would_borrow_on_as_written_stays_plain() {
        // `replace` is `RefCell::replace` here; as sugar it would go through a borrow.
        let source = with_uses(
            "fn f(n: Rc<RefCell<u8>>) -> u8 {\n    let a = *n.borrow();\n    n.replace(a)\n}\n\
             fn g() -> u8 {\n    let x = Rc::new(RefCell::new(1));\n    \
             x.borrow().pow(2) + x.replace(2)\n}\n",
        );
        assert_eq!(sugar(&source).unwrap(), source);
    }

    #[test]
    fn a_borrow_inside_another_rc_value_keeps_the_binding_first() {
        // The removal and the inner `rc` edit overlap; the binding's set comes first and wins.
        assert_sugar(
            &with_uses(
                "fn f(n: Rc<RefCell<Node>>) -> Rc<RefCell<u64>> {\n    \
                 Rc::new(RefCell::new(n.borrow().size))\n}\n",
            ),
            &with_uses(
                "fn f(n: rc Node) -> rc u64 {\n    \
                 Rc::new(RefCell::new(n.size))\n}\n",
            ),
        );
    }

    #[test]
    fn a_shadowing_let_or_closure_parameter_of_the_name_keeps_every_borrow() {
        // The pass's scope ends at the new binding, so it would not add back the borrow after it;
        // these are conservative misses the test pins.
        keeps_borrows(
            "fn f(n: Rc<RefCell<Node>>) -> u64 {\n    let a = n.borrow().size;\n    \
             let n = other();\n    a + n.borrow().size\n}\n",
            "fn f(n: rc Node) -> u64 {\n    let a = n.borrow().size;\n    \
             let n = other();\n    a + n.borrow().size\n}\n",
        );
        keeps_borrows(
            "fn f(n: Rc<RefCell<Node>>, x: RefCell<Node>) -> u64 {\n    \
             let g = |n: &RefCell<Node>| n.borrow().size;\n    n.borrow().size + g(&x)\n}\n",
            "fn f(n: rc Node, x: RefCell<Node>) -> u64 {\n    \
             let g = |n: &RefCell<Node>| n.borrow().size;\n    n.borrow().size + g(&x)\n}\n",
        );
    }

    /// Rewrites the bytes of `0` as they are, starting before any other rule's edit there.
    struct Cover(&'static str);

    impl ReverseRule for Cover {
        fn edits(&self, _: &syn::File, source: &str) -> Vec<Edit> {
            let at = source.find(self.0).unwrap();
            vec![Edit {
                range: at..at + self.0.len(),
                text: self.0.to_owned(),
            }]
        }
    }

    #[test]
    fn the_removals_are_kept_or_dropped_with_the_binding_edit() {
        let typed = with_uses("fn f(n: Rc<RefCell<Node>>) -> u64 {\n    n.borrow().size\n}\n");
        let file = syn::parse_file(&typed).unwrap();
        let [group] = &PrefixTypes.edit_groups(&file, &typed)[..] else {
            panic!("one set of edits")
        };
        assert_eq!(group.len(), 2, "the type and its borrow in one set");
        // An earlier edit over the type wins, so the borrow stays with the type.
        let rules: [&dyn ReverseRule; 2] = [&Cover("n: Rc<RefCell<Node>>"), &PrefixTypes];
        assert_eq!(convert(&typed, &rules).unwrap(), typed);

        let value = with_uses(
            "fn f() -> usize {\n    let v = Rc::new(RefCell::new(Vec::new()));\n    v.borrow().len()\n}\n",
        );
        let file = syn::parse_file(&value).unwrap();
        let [group] = &RcValues.edit_groups(&file, &value)[..] else {
            panic!("one set of edits")
        };
        assert_eq!(group.len(), 2, "the value and its borrow in one set");
        let rules: [&dyn ReverseRule; 2] = [&Cover("v = Rc::new"), &RcValues];
        assert_eq!(convert(&value, &rules).unwrap(), value);
    }
}
