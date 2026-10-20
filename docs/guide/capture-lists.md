# `move[..]` capture lists

`move[a.clone(), b] || body` binds each capture before a `move` closure, keeping the names. Problem: [11, cloning for closures](../../problems/11-closure-clones.md).

```rust
rustx::sugar! {
    use std::sync::Arc;

    fn counter(name: Arc<String>, log: Arc<Vec<String>>) -> impl Fn() -> usize {
        move[name.clone(), log.clone()] || name.len() + log.len()
    }
}

let name = Arc::new("ann".to_string());
let log = Arc::new(vec!["x".to_string()]);
let count = counter(name.clone(), log);
assert_eq!(count(), 4);
assert_eq!(name.len(), 3);
```

Becomes:

```rust
use std::sync::Arc;
fn counter(name: Arc<String>, log: Arc<Vec<String>>) -> impl Fn() -> usize {
    {
        let name = name.clone();
        let log = log.clone();
        move || name.len() + log.len()
    }
}
```

`async move[..] || ..` works the same way, and lists nest. The form stays close to `move(expr)` (RFC 3968), so it can switch to the native syntax once that is stable.

[`rustx-cli sugar`](cli.md) turns a block of `let a = a.clone();` bindings before a `move` closure back into `move[a.clone()] || ..`.

## Limits

- A capture binds the variable it starts with, followed only through method calls (`a`, `a.clone()`, `a.lock().unwrap().clone()`). Field access, `&a`, paths and `self` are errors, as are an empty list and a name captured twice.
- The list must be followed by a closure: `move[..] async ||` and async blocks are errors.
- Like `var`, it is rewritten inside macro-call arguments too.
