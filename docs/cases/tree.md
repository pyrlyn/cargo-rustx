# Directory tree

A directory owns its children through shared mutable nodes. Ordinary Rust writes `Rc<RefCell<_>>` and `.borrow()` / `.borrow_mut()` on every field. [`rc`](../guide/rc.md) and [`into`](../guide/into-params.md) keep the tree as field access. A string literal in a `String` field is [converted](../guide/string-literals.md).

## Plain Rust

```rust
use std::cell::RefCell;
use std::rc::Rc;

struct Dir {
    name: String,
    files: Vec<String>,
    kids: Vec<Rc<RefCell<Dir>>>,
}

fn add_file(dir: &Rc<RefCell<Dir>>, name: impl Into<String>) {
    dir.borrow_mut().files.push(name.into());
}

fn nest(root: &Rc<RefCell<Dir>>) -> (usize, usize) {
    let child = Rc::new(RefCell::new(Dir {
        name: "src".to_string(),
        files: Vec::new(),
        kids: Vec::new(),
    }));
    add_file(&child, "main.rs");
    let files = child.borrow().files.len();
    root.borrow_mut().kids.push(child);
    (root.borrow().kids.len(), files)
}

fn demo() -> (usize, usize) {
    let root = Rc::new(RefCell::new(Dir {
        name: "app".to_string(),
        files: Vec::new(),
        kids: Vec::new(),
    }));
    nest(&root)
}

assert_eq!(demo(), (1, 1));
```

## With rustx

```rust
rustx::sugar! {
    struct Dir {
        name: String,
        files: Vec<String>,
        kids: Vec<rc Dir>,
    }

    fn add_file(dir: &rc Dir, name: into String) {
        dir.files.push(name);
    }

    fn nest(root: &rc Dir) -> (usize, usize) {
        let child = rc Dir { name: "src", files: Vec::new(), kids: Vec::new() };
        add_file(&child, "main.rs");
        let files = child.files.len();
        root.kids.push(child);
        (root.kids.len(), files)
    }

    fn demo() -> (usize, usize) {
        let root = rc Dir { name: "app", files: Vec::new(), kids: Vec::new() };
        nest(&root)
    }
}

assert_eq!(demo(), (1, 1));
```
