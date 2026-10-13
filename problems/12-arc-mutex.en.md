# `Arc<Mutex<T>>` and lock noise

## As is (Rust)

```rust
let s = Arc::new(Mutex::new(State::default()));
s.lock().unwrap().count += 1;
```

## Desired (sugar)

```rust
shared s = State::default();
s.count += 1;
```

## Implementation

Wrapper type plus macro inserting `.lock().unwrap()` (or a `with` closure) on access.
