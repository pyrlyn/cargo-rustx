# `Arc<Mutex<T>>` и lock-шум

## Как есть (Rust)

```rust
let s = Arc::new(Mutex::new(State::default()));
s.lock().unwrap().count += 1;
```

## Как хочется

```rust
shared s = State::default();
s.count += 1;
```

## Реализация

Тип-обёртка и макрос, вставляющий `.lock().unwrap()` (или замыкание `with`) при доступе.
