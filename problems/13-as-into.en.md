# Explicit `as` and `into()`

## As is (Rust)

```rust
let x: u64 = (a as u64) + b.into();
```

## Desired (sugar)

```rust
let x: u64 = a + b;
```

## Implementation

Expression macro inserting `into()` for lossless widening conversions only.
