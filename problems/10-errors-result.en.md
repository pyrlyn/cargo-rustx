# Errors and `Result`

## As is (Rust)

```rust
fn load() -> Result<Config, Box<dyn std::error::Error + Send + Sync>>
```

## Desired (sugar)

```rust
fn load() -> Config throws
```

## Implementation

Attribute macro wrapping the return type in `anyhow::Result<T>` and the tail expression in `Ok(..)`.
