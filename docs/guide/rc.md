# `rc T`, `weak T` and `rc e`

`rc T` is `Rc<RefCell<T>>`, `weak T` is `Weak<RefCell<T>>`, `rc e` is `Rc::new(RefCell::new(e))`, and field access on a binding typed `rc T` borrows for you. Problem: [16, `Rc<RefCell<T>>` in trees and graphs](../../problems/16-rc-refcell.md).

```rust
rustx::sugar! {
    struct Node {
        value: i32,
        children: Vec<rc Node>,
        parent: Option<weak Node>,
    }

    fn grow(root: &rc Node) -> i32 {
        let child = rc Node { value: 2, children: Vec::new(), parent: Some(std::rc::Rc::downgrade(root)) };
        root.children.push(child);
        root.value += 1;
        root.value
    }
}

let root = std::rc::Rc::new(std::cell::RefCell::new(Node { value: 1, children: Vec::new(), parent: None }));
assert_eq!(grow(&root), 2);
assert_eq!(root.borrow().children.len(), 1);
```

Becomes:

```rust
struct Node {
    value: i32,
    children: Vec<::std::rc::Rc<::core::cell::RefCell<Node>>>,
    parent: Option<::std::rc::Weak<::core::cell::RefCell<Node>>>,
}
fn grow(root: &::std::rc::Rc<::core::cell::RefCell<Node>>) -> i32 {
    let child = ::std::rc::Rc::new(
        ::std::cell::RefCell::new(Node {
            value: 2,
            children: Vec::new(),
            parent: Some(std::rc::Rc::downgrade(root)),
        }),
    );
    root.borrow_mut().children.push(child);
    root.borrow_mut().value += 1;
    root.borrow().value
}
```

A write (assignment, compound assignment, `&mut`, or a known `&mut self` std method such as `push`) borrows mutably, anything else immutably. A statement with several accesses opens one guard, mutable if one of them writes. For a user `&mut self` method, `use rustx::RcExt;` and call `n.with_mut(|n| ..)`.

## From plain Rust

[`rustx-cli sugar`](cli.md#sugar-move-plain-rust-onto-sugar) turns `Rc::new(RefCell::new(e))` into `rc e` and `Rc<RefCell<T>>` into `rc T`. On a binding it converts (a `let x = Rc::new(RefCell::new(e))`, or a `let`, fn or closure parameter typed `Rc<RefCell<T>>` or `&Rc<RefCell<T>>`), it also removes the explicit `.borrow()` and `.borrow_mut()` on field accesses and method calls, when the borrows the sugar adds back are exactly those: `n.borrow_mut().items.push(x)` becomes `n.items.push(x)`. It removes all of a binding's borrows or none: a binding with any other borrow of it (a held guard `let g = n.borrow();`, `*n.borrow_mut() = v`, `try_borrow`, a borrow passed to a call) or with a borrow the sugar would write differently (two accesses in one statement, which share one guard, or `n.borrow_mut().grow()` for a user method the sugar would borrow immutably) converts but keeps every borrow as written. A binding the sugar would borrow on as written, such as `n.replace(v)` meaning `RefCell::replace`, stays `Rc<RefCell<T>>`.

## Limits

- Only bindings are tracked: `let`, fn and closure parameters whose type is written `rc T` or `&rc T`, and `let x = rc e`. Fields typed `rc T`, bindings without a written type, bindings declared inside a macro call and hand-written `Rc<RefCell<T>>` keep explicit borrows.
- A user `&mut self` method gets `.borrow()` and fails to compile; use `with_mut`. `RefCell` still panics at run time on conflicting borrows, including against a `Ref` that `let r = &n.items;` keeps alive.
- With `std::borrow::Borrow` or `BorrowMut` imported, the inserted calls resolve to those traits, as hand-written ones would.
- `rc e` needs a value that cannot follow a name in valid Rust (`rc x`, `rc 0`, `rc vec![..]`, `rc Node { .. }`); `rc &x`, `rc -1`, `rc (x)` and `rc |a| ..` stay valid Rust with `rc` as a name. The value runs to the next `,` or `;`, like the operand of `return`.
- `rc` and `weak` are sugar in types only in a recognisable type position. A struct literal field `S { f: rc x }` with a bare name reads as a type: write `S { f: (rc x) }`.
