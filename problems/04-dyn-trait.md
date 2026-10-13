# `dyn Trait` в сигнатурах

## Как есть (Rust)

```rust
fn handlers() -> Vec<Box<dyn Fn(&Req) -> Resp + Send + Sync>>
```

## Как хочется

```rust
fn handlers() -> Vec<~Fn(&Req) -> Resp>
```

## Реализация

Десугаринг `~Trait` в `Box<dyn Trait + Send + Sync>` (набор ограничений настраивается).
