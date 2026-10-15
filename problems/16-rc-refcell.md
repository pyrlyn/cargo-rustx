# `Rc<RefCell<T>>` in trees and graphs

## As is (Rust)

```rust
children: Vec<Rc<RefCell<Node>>>,
parent: Option<Weak<RefCell<Node>>>,
node.borrow_mut().children.push(Rc::new(RefCell::new(child)));
```

## Desired (sugar)

```rust
children: Vec<rc Node>,
parent: Option<weak Node>,
node.children.push(rc child);
```

## Implementation

The single-threaded twin of `shared` (item 12). Types: `rc T` → `Rc<RefCell<T>>`, `weak T` → `Weak<RefCell<T>>`. Expressions: `rc e` → `Rc::new(RefCell::new(e))`. On a binding whose type is written as `rc T` or `&rc T`, field access gets `.borrow_mut()` when the field is assigned, compound-assigned, or receives a call to a known `&mut self` std method (`push`, `insert`, `remove`, `clear`, `extend`, `pop`, `retain`, `sort`, ...), and `.borrow()` otherwise.

## Detailed example

### As is (valid Rust)

```rust
use std::cell::RefCell;
use std::rc::{Rc, Weak};

pub struct Node {
    pub name: String,
    pub size: u64,
    pub parent: Option<Weak<RefCell<Node>>>,
    pub children: Vec<Rc<RefCell<Node>>>,
}

pub fn new_node(name: &str, size: u64) -> Rc<RefCell<Node>> {
    Rc::new(RefCell::new(Node {
        name: name.to_string(),
        size,
        parent: None,
        children: Vec::new(),
    }))
}

pub fn add_child(parent: &Rc<RefCell<Node>>, child: Rc<RefCell<Node>>) {
    child.borrow_mut().parent = Some(Rc::downgrade(parent));
    parent.borrow_mut().size += child.borrow().size;
    parent.borrow_mut().children.push(child);
}

pub fn path(node: &Rc<RefCell<Node>>) -> String {
    let mut parts = vec![node.borrow().name.clone()];
    let mut cur = node.borrow().parent.as_ref().and_then(|w| w.upgrade());
    while let Some(p) = cur {
        parts.push(p.borrow().name.clone());
        cur = p.borrow().parent.as_ref().and_then(|w| w.upgrade());
    }
    parts.reverse();
    parts.join("/")
}

pub fn count(node: &Rc<RefCell<Node>>) -> usize {
    1 + node.borrow().children.iter().map(count).sum::<usize>()
}

pub fn demo() -> (String, usize, u64) {
    let root = new_node("root", 0);
    let src = new_node("src", 0);
    let main = new_node("main.rs", 120);
    add_child(&src, main.clone());
    add_child(&root, src);
    let total = root.borrow().size;
    (path(&main), count(&root), total)
}
```

### Desired (cargo-rustx)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugared] on an item cannot be used
sugar! {
mod tree {
    use std::rc::Rc;

    pub struct Node {
        pub name: String,
        pub size: u64,
        pub parent: Option<weak Node>,
        pub children: Vec<rc Node>,
    }

    pub fn new_node(name: &str, size: u64) -> rc Node {
        rc Node { name: name.to_string(), size, parent: None, children: Vec::new() }
    }

    pub fn add_child(parent: &rc Node, child: rc Node) {
        child.parent = Some(Rc::downgrade(parent));
        parent.size += child.size;
        parent.children.push(child);
    }

    pub fn path(node: &rc Node) -> String {
        var parts = vec![node.name.clone()];
        var cur = node.parent.as_ref().and_then(|w| w.upgrade());
        while let Some(p) = cur {
            // `p` has no written type, so its borrows stay explicit
            parts.push(p.borrow().name.clone());
            cur = p.borrow().parent.as_ref().and_then(|w| w.upgrade());
        }
        parts.reverse();
        parts.join("/")
    }

    pub fn count(node: &rc Node) -> usize {
        1 + node.children.iter().map(count).sum<usize>()
    }

    pub fn demo() -> (String, usize, u64) {
        let root = new_node("root", 0);
        let src = new_node("src", 0);
        let main = new_node("main.rs", 120);
        add_child(&src, main.clone());
        add_child(&root, src);
        let total = root.borrow().size;   // `root` has no written type either
        (path(&main), count(&root), total)
    }
}
}
```

### What changed

- `Rc<RefCell<Node>>` and `Weak<RefCell<Node>>` became `rc Node` and `weak Node`. `Rc::new(RefCell::new(..))` became `rc ..`.
- On bindings typed `rc T`, `.borrow()` and `.borrow_mut()` are inserted per access, the same way `shared` inserts `.lock().unwrap()`.
- The sugar does not change `RefCell` semantics. Each access is a separate borrow, and a read while a write borrow is alive still panics at run time. Use `node.with_mut(|n| ..)` for a group of operations.
- Also: `var`, `sum<usize>()` without turbofish.
