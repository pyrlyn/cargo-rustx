# Turbofish

## Как есть (Rust)

```rust
let n = s.parse::<u32>()?;
let v = it.collect::<Vec<_>>();
```

## Как хочется

```rust
let n = s.parse<u32>()?;
let v = it.collect<Vec>();
```

## Реализация

Переписывание токенов `.method<T>(` в `.method::<T>(` с достройкой недостающих `<_>`.
