# Cloning for closures

## As is (Rust)

```rust
let a = a.clone();
let b = b.clone();
spawn(move || work(a, b));
```

## Desired (sugar)

```rust
spawn(move[a.clone(), b.clone()] || work(a, b));
```

## Implementation

Capture-list macro expanding into `let` bindings before a `move` closure.
