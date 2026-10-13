# Ошибки и `Result`

## Как есть (Rust)

```rust
fn load() -> Result<Config, Box<dyn std::error::Error + Send + Sync>>
```

## Как хочется

```rust
fn load() -> Config throws
```

## Реализация

Атрибутный макрос оборачивает тип в `anyhow::Result<T>`, а хвостовое выражение в `Ok(..)`.
