# `async` with `Pin` and `Box` in traits

## As is (Rust)

```rust
fn call(&self) -> Pin<Box<dyn Future<Output = T> + Send + '_>>
```

## Desired (sugar)

```rust
async fn call(&self) -> T
```

## Implementation

Native async fn in traits, plus `async_trait`-style boxing macro for `dyn` use.
