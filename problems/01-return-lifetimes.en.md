# Lifetimes in return types

## As is (Rust)

```rust
fn first<'a>(xs: &'a [Item], key: &str) -> &'a Item
```

## Desired (sugar)

```rust
fn first(xs: &[Item], key: &str) -> &from(xs) Item
```

## Implementation

Attribute macro that introduces a named lifetime and binds it to the parameter referenced by `from(..)`.
