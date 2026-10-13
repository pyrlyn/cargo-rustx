# Вездесущий `mut`

## Как есть (Rust)

```rust
let mut v = Vec::new();
let mut i = 0;
```

## Как хочется

```rust
var v = Vec::new();
var i = 0;
```

## Реализация

Макрос на функцию или блок переписывает `var x = ..` в `let mut x = ..`.
