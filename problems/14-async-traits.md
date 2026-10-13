# `async` с `Pin` и `Box` в трейтах

## Как есть (Rust)

```rust
fn call(&self) -> Pin<Box<dyn Future<Output = T> + Send + '_>>
```

## Как хочется

```rust
async fn call(&self) -> T
```

## Реализация

Нативный async fn в трейтах плюс макрос боксинга в стиле `async_trait` для `dyn`.
