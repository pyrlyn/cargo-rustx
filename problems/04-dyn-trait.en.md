# `dyn Trait` in signatures

## As is (Rust)

```rust
fn handlers() -> Vec<Box<dyn Fn(&Req) -> Resp + Send + Sync>>
```

## Desired (sugar)

```rust
fn handlers() -> Vec<~Fn(&Req) -> Resp>
```

## Implementation

Desugar `~Trait` into `Box<dyn Trait + Send + Sync>` (configurable bounds).
