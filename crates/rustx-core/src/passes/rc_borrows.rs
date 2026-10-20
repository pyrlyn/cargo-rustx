//! Borrows on `rc T` field access (problem 16, `problems/16-rc-refcell.md`).
//!
//! On a binding whose type is written `rc T` or `&rc T`, or that a `let` binds to an `rc e`
//! value, field access and value-method calls go through the `RefCell`: `.borrow_mut()` when the
//! access writes (assignment, compound assignment, `&mut`, a known `&mut self` std method) and
//! `.borrow()` otherwise. Units and guards are those of `access`: a statement with one access
//! borrows for it, `n.borrow().size`; one with more shares one guard,
//! `{ let __rustx_n = &mut *n.borrow_mut(); .. }`, mutable if any of them writes, because a
//! `RefCell` panics when a second borrow meets a live mutable one. Handle methods (`borrow`,
//! `borrow_mut`, `try_borrow`, `try_borrow_mut`, `with`, `with_mut`, `clone`) and bare uses such
//! as `Rc::downgrade(n)` stay as written.
//!
//! Tracked bindings: `let` bindings, fn and method parameters, and closure parameters. Fields
//! typed `rc T`, bindings without a written type (`let p = w.upgrade()`) and bindings declared
//! inside a macro call keep explicit borrows.
//!
//! # Telling the sugar from plain Rust
//!
//! `#[sugared]` runs this pass on valid Rust, and the plain spelling of `rc T` and `rc e` is one a
//! user may write by hand, where `n.replace(x)` means `RefCell::replace` and must stay. So
//! `prefix_types` and `rc_values` start their expansions with the reserved segment
//! [`MARKER`] (`__rustx_rc::std::rc::Rc<..>`): only bindings whose type or value carries it are
//! tracked, and this pass then removes it everywhere, macro bodies included.

use std::mem;

use proc_macro2::{Group, Ident, Span, TokenStream, TokenTree};
use syn::punctuated::Pair;
use syn::visit_mut::{self, VisitMut};
use syn::{
    Block, Expr, ExprClosure, FnArg, ImplItemFn, ItemFn, Local, Macro, MetaList, Pat, PatIdent,
    PatType, Path, Signature, Stmt, TraitItemFn, Type, parse_quote_spanned,
};

use super::access::{self, Opening};

/// The first path segment of a marked `rc` expansion.
pub(super) const MARKER: &str = "__rustx_rc";

const BORROW: Opening = Opening {
    open: borrow,
    handle_methods: &[
        "borrow",
        "borrow_mut",
        "try_borrow",
        "try_borrow_mut",
        "with",
        "with_mut",
    ],
};

// Reads take a shared borrow, so they do not panic against other live reads, such as a `Ref`
// the caller still holds.
fn borrow(name: &Ident, writes: bool, guard: bool) -> Expr {
    let span = name.span();
    match (writes, guard) {
        (true, true) => parse_quote_spanned!(span=> &mut *#name.borrow_mut()),
        (true, false) => parse_quote_spanned!(span=> #name.borrow_mut()),
        (false, true) => parse_quote_spanned!(span=> &*#name.borrow()),
        (false, false) => parse_quote_spanned!(span=> #name.borrow()),
    }
}

/// The marker segment for an expansion of the sugar at `span`.
pub(super) fn marker(span: Span) -> Ident {
    Ident::new(MARKER, span)
}

// The pass finds no misuse: an untracked binding is plain Rust, which rustc checks. The
// signature is the one every AST pass has.
#[allow(clippy::unnecessary_wraps)]
pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    Lowering.visit_file_mut(file);
    Ok(())
}

struct Lowering;

impl VisitMut for Lowering {
    fn visit_item_fn_mut(&mut self, item: &mut ItemFn) {
        borrow_params(&item.sig, &mut item.block.stmts);
        visit_mut::visit_item_fn_mut(self, item);
    }

    fn visit_impl_item_fn_mut(&mut self, item: &mut ImplItemFn) {
        borrow_params(&item.sig, &mut item.block.stmts);
        visit_mut::visit_impl_item_fn_mut(self, item);
    }

    fn visit_trait_item_fn_mut(&mut self, item: &mut TraitItemFn) {
        if let Some(body) = &mut item.default {
            borrow_params(&item.sig, &mut body.stmts);
        }
        visit_mut::visit_trait_item_fn_mut(self, item);
    }

    fn visit_expr_closure_mut(&mut self, closure: &mut ExprClosure) {
        let names: Vec<Ident> = closure
            .inputs
            .iter()
            .filter_map(|pat| match pat {
                Pat::Type(typed) => typed_name(typed).cloned(),
                _ => None,
            })
            .collect();
        if !names.is_empty() {
            // `access` works on statements; a closure body is one expression.
            let body = mem::replace(&mut *closure.body, Expr::Verbatim(TokenStream::new()));
            let mut scope = [Stmt::Expr(body, None)];
            for name in &names {
                access::rewrite(name, &mut scope, &BORROW);
            }
            let [Stmt::Expr(body, _)] = scope else {
                unreachable!("`access` keeps an expression statement one")
            };
            *closure.body = body;
        }
        visit_mut::visit_expr_closure_mut(self, closure);
    }

    fn visit_block_mut(&mut self, block: &mut Block) {
        for i in 0..block.stmts.len() {
            let (head, scope) = block.stmts.split_at_mut(i + 1);
            if let Stmt::Local(local) = &head[i]
                && let Some(name) = local_name(local)
            {
                access::rewrite(name, scope, &BORROW);
            }
        }
        visit_mut::visit_block_mut(self, block);
    }

    fn visit_path_mut(&mut self, path: &mut Path) {
        visit_mut::visit_path_mut(self, path);
        if marked(path) {
            let mut segments = mem::take(&mut path.segments).into_pairs();
            if let Some(Pair::Punctuated(_, colons)) = segments.next() {
                path.leading_colon = Some(colons);
            }
            path.segments = segments.collect();
        }
    }

    fn visit_macro_mut(&mut self, mac: &mut Macro) {
        visit_mut::visit_macro_mut(self, mac);
        if has_marker(&mac.tokens) {
            mac.tokens = unmark(mem::take(&mut mac.tokens));
        }
    }

    fn visit_meta_list_mut(&mut self, list: &mut MetaList) {
        visit_mut::visit_meta_list_mut(self, list);
        if has_marker(&list.tokens) {
            list.tokens = unmark(mem::take(&mut list.tokens));
        }
    }
}

/// Whether this pass would add borrows to `scope` if `name` were an `rc` binding, so a reverse
/// rule can leave such a binding's type plain without expanding the file.
#[cfg(feature = "sugar")]
pub(crate) fn would_borrow(name: &Ident, scope: &[Stmt]) -> bool {
    access::scan(name, &mut scope.to_vec(), &BORROW).accesses
}

/// Adds to `scope` the borrows this pass adds for an `rc` binding `name`, so a reverse rule can
/// check that it removes exactly those.
#[cfg(feature = "sugar")]
pub(crate) fn add_borrows(name: &Ident, scope: &mut [Stmt]) {
    access::rewrite(name, scope, &BORROW);
}

fn borrow_params(sig: &Signature, body: &mut [Stmt]) {
    for input in &sig.inputs {
        if let FnArg::Typed(typed) = input
            && let Some(name) = typed_name(typed)
        {
            access::rewrite(name, body, &BORROW);
        }
    }
}

/// The name a `let` binds to an `rc` value: its written type is `rc T` or `&rc T`, or it has no
/// written type and its value is `rc e`.
fn local_name(local: &Local) -> Option<&Ident> {
    if let Pat::Type(typed) = &local.pat {
        return typed_name(typed);
    }
    let init = local.init.as_ref()?;
    let rc_value = matches!(&*init.expr, Expr::Call(call)
        if matches!(&*call.func, Expr::Path(func) if func.qself.is_none() && marked(&func.path)));
    rc_value.then(|| single_name(&local.pat)).flatten()
}

/// The name of `name: rc T` or `name: &rc T`.
fn typed_name(typed: &PatType) -> Option<&Ident> {
    let ty = match &*typed.ty {
        Type::Reference(reference) => &*reference.elem,
        ty => ty,
    };
    let rc_type = matches!(ty, Type::Path(path) if path.qself.is_none() && marked(&path.path));
    rc_type.then(|| single_name(&typed.pat)).flatten()
}

fn single_name(pat: &Pat) -> Option<&Ident> {
    match pat {
        Pat::Ident(PatIdent {
            ident,
            subpat: None,
            ..
        }) => Some(ident),
        _ => None,
    }
}

fn marked(path: &Path) -> bool {
    path.leading_colon.is_none()
        && path.segments.len() > 1
        && path
            .segments
            .first()
            .is_some_and(|first| first.ident == MARKER)
}

fn has_marker(tokens: &TokenStream) -> bool {
    tokens.clone().into_iter().any(|tree| match tree {
        TokenTree::Ident(ident) => ident == MARKER,
        TokenTree::Group(group) => has_marker(&group.stream()),
        _ => false,
    })
}

/// `tokens` without the marker, for bodies `syn` leaves as tokens. The `::` after it stays and
/// becomes the leading `::` of the plain path.
pub(super) fn unmark(tokens: TokenStream) -> TokenStream {
    tokens
        .into_iter()
        .filter_map(|tree| match tree {
            TokenTree::Ident(ident) if ident == MARKER => None,
            // Rebuilding a group resets its delimiter spans, so only groups that need it are.
            TokenTree::Group(group) if has_marker(&group.stream()) => {
                let mut plain = Group::new(group.delimiter(), unmark(group.stream()));
                plain.set_span(group.span());
                Some(TokenTree::Group(plain))
            }
            tree => Some(tree),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::{ToTokens, quote};

    fn expand(sugar: TokenStream) -> String {
        crate::try_expand(sugar)
            .unwrap()
            .into_token_stream()
            .to_string()
    }

    fn expands_to(sugar: TokenStream, plain: TokenStream) {
        let plain = syn::parse2::<syn::File>(plain).unwrap();
        assert_eq!(expand(sugar), plain.into_token_stream().to_string());
    }

    #[test]
    fn rc_parameters_borrow_mutably_only_where_the_access_writes() {
        expands_to(
            quote! { fn add_child(parent: &rc Node, child: rc Node) {
                child.parent = Some(Rc::downgrade(parent));
                parent.size += child.size;
                parent.children.push(child);
                let n = parent.children.len();
            } },
            quote! { fn add_child(
                parent: &::std::rc::Rc<::core::cell::RefCell<Node> >,
                child: ::std::rc::Rc<::core::cell::RefCell<Node> >
            ) {
                child.borrow_mut().parent = Some(Rc::downgrade(parent));
                parent.borrow_mut().size += child.borrow().size;
                parent.borrow_mut().children.push(child);
                let n = parent.borrow().children.len();
            } },
        );
    }

    #[test]
    fn accesses_of_one_statement_share_one_guard_that_is_mutable_if_one_writes() {
        expands_to(
            quote! { fn f(n: rc Node) -> (u64, usize) {
                n.size = n.size * 2;
                for c in n.children.iter() { n.seen += 1; }
                (n.size, n.children.len())
            } },
            quote! { fn f(n: ::std::rc::Rc<::core::cell::RefCell<Node> >) -> (u64, usize) {
                { let __rustx_n = &mut *n.borrow_mut(); __rustx_n.size = __rustx_n.size * 2 };
                { let __rustx_n = &mut *n.borrow_mut(); for c in __rustx_n.children.iter() { __rustx_n.seen += 1; } }
                { let __rustx_n = &*n.borrow(); (__rustx_n.size, __rustx_n.children.len()) }
            } },
        );
    }

    #[test]
    fn lets_typed_rc_or_bound_to_an_rc_value_and_closure_parameters_are_tracked() {
        expands_to(
            quote! { fn f(other: &rc Node) {
                let a: rc Node = make();
                var b = rc Vec::new();
                let r: &rc Node = other;
                a.size = r.size;
                b.push(1);
                let sizes = list.iter().map(|c: &rc Node| c.size + c.extra);
            } },
            quote! { fn f(other: &::std::rc::Rc<::core::cell::RefCell<Node> >) {
                let a: ::std::rc::Rc<::core::cell::RefCell<Node> > = make();
                let mut b = ::std::rc::Rc::new(::std::cell::RefCell::new(Vec::new()));
                let r: &::std::rc::Rc<::core::cell::RefCell<Node> > = other;
                a.borrow_mut().size = r.borrow().size;
                b.borrow_mut().push(1);
                let sizes = list.iter().map(|c: &::std::rc::Rc<::core::cell::RefCell<Node> >| {
                    let __rustx_c = &*c.borrow();
                    __rustx_c.size + __rustx_c.extra
                });
            } },
        );
    }

    #[test]
    fn handle_methods_bare_uses_and_rebinding_clones_stay_on_the_handle() {
        expands_to(
            quote! { fn f(n: rc Node) {
                n.borrow_mut().size = 1;
                let w = Rc::downgrade(&n);
                let s = n.with(|v| v.size);
                n.with_mut(|v| v.rename());
                let t = n.try_borrow().is_ok();
                let m = n.try_borrow_mut().is_ok();
                let k = move[n.clone()] || n.size;
                g(n);
            } },
            quote! { fn f(n: ::std::rc::Rc<::core::cell::RefCell<Node> >) {
                n.borrow_mut().size = 1;
                let w = Rc::downgrade(&n);
                let s = n.with(|v| v.size);
                n.with_mut(|v| v.rename());
                let t = n.try_borrow().is_ok();
                let m = n.try_borrow_mut().is_ok();
                let k = { let n = n.clone(); move || n.borrow().size };
                g(n);
            } },
        );
    }

    #[test]
    fn bindings_without_a_written_rc_type_or_value_are_not_tracked() {
        let valid = quote! { fn f(w: Weak<RefCell<Node> >, v: Vec<::std::rc::Rc<::core::cell::RefCell<Node> > >) {
            let p = w.upgrade().unwrap();
            let q = p.clone();
            let r = Rc::new(RefCell::new(Node::default()));
            p.borrow().size;
            for c in v { c.borrow().size; }
        } };
        expands_to(valid.clone(), valid);
    }

    #[test]
    fn hand_written_rc_refcell_is_left_alone_even_in_the_emitted_spelling() {
        // `#[sugared]` runs the AST passes on valid Rust, where `replace` is `RefCell::replace`.
        let valid = quote! {
            fn f(n: &::std::rc::Rc<::core::cell::RefCell<Vec<u8> > >) -> Vec<u8> {
                let m = ::std::rc::Rc::new(::std::cell::RefCell::new(1));
                m.replace(2);
                n.replace(vec![1])
            }
        };
        let mut file: syn::File = syn::parse2(valid.clone()).unwrap();
        lower(&mut file).unwrap();
        assert_eq!(file.into_token_stream().to_string(), valid.to_string());
        expands_to(valid.clone(), valid);
    }

    #[test]
    fn the_marker_is_removed_from_every_type_value_and_macro_body() {
        expands_to(
            quote! {
                struct Node { children: Vec<rc Node> }
                fn f(x: Node) -> rc Node {
                    let v: Vec<rc Node> = vec![rc x, rc Node::default()];
                    assert!(matches!(Some(rc 1), Some(_)));
                    rc x
                }
            },
            quote! {
                struct Node { children: Vec<::std::rc::Rc<::core::cell::RefCell<Node> > > }
                fn f(x: Node) -> ::std::rc::Rc<::core::cell::RefCell<Node> > {
                    let v: Vec<::std::rc::Rc<::core::cell::RefCell<Node> > > = vec![
                        ::std::rc::Rc::new(::std::cell::RefCell::new(x)),
                        ::std::rc::Rc::new(::std::cell::RefCell::new(Node::default()))
                    ];
                    assert!(matches!(Some(::std::rc::Rc::new(::std::cell::RefCell::new(1))), Some(_)));
                    ::std::rc::Rc::new(::std::cell::RefCell::new(x))
                }
            },
        );
    }

    #[test]
    fn the_rc_expansions_carry_the_marker_until_this_pass() {
        let tokens = crate::passes::rewrite_tokens(quote! { fn f(a: rc u8) { let b = rc 1; } });
        let tokens = tokens.unwrap();
        assert!(has_marker(&tokens));
        assert!(!has_marker(&unmark(tokens)));
    }

    #[test]
    fn methods_parameters_and_trait_default_bodies_are_tracked() {
        expands_to(
            quote! {
                impl S { fn m(&self, n: rc Node) { n.size += 1; } }
                trait T { fn d(n: &rc Node) -> u64 { n.size } }
            },
            quote! {
                impl S { fn m(&self, n: ::std::rc::Rc<::core::cell::RefCell<Node> >) { n.borrow_mut().size += 1; } }
                trait T { fn d(n: &::std::rc::Rc<::core::cell::RefCell<Node> >) -> u64 { n.borrow().size } }
            },
        );
    }
}
