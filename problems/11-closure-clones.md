# Клонирование в замыканиях

## Как есть (Rust)

```rust
let a = a.clone();
let b = b.clone();
spawn(move || work(a, b));
```

## Как хочется

```rust
spawn(move[a.clone(), b.clone()] || work(a, b));
```

## Реализация

Макрос списка захвата разворачивается в `let`-привязки перед `move`-замыканием.
