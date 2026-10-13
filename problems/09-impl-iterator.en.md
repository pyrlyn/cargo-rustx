# `impl Trait` and complex iterator returns

## As is (Rust)

```rust
fn evens(v: &[i32]) -> impl Iterator<Item = &i32> + '_
```

## Desired (sugar)

```rust
fn evens(v: &[i32]) -> iter &i32
```

## Implementation

Desugar `iter T` into `impl Iterator<Item = T> + '_`.
