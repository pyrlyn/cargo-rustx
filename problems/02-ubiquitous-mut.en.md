# Ubiquitous `mut`

## As is (Rust)

```rust
let mut v = Vec::new();
let mut i = 0;
```

## Desired (sugar)

```rust
var v = Vec::new();
var i = 0;
```

## Implementation

Function/block macro rewriting `var x = ..` into `let mut x = ..`.
