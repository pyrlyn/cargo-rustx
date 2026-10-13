# Длинные сигнатуры с generics и `where`

## Как есть (Rust)

```rust
fn run<T, E>(x: T) -> Result<T, E>
where
    T: Clone + Debug + Send,
    E: Error,
```

## Как хочется

```rust
fn run(x: T: Clone + Debug + Send) -> Result<T, E: Error>
```

## Реализация

Атрибутный макрос выносит inline-ограничения в список generics и `where`.
