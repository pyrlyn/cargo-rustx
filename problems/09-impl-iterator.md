# `impl Trait` и сложные возвращаемые итераторы

## Как есть (Rust)

```rust
fn evens(v: &[i32]) -> impl Iterator<Item = &i32> + '_
```

## Как хочется

```rust
fn evens(v: &[i32]) -> iter &i32
```

## Реализация

Десугаринг `iter T` в `impl Iterator<Item = T> + '_`.
