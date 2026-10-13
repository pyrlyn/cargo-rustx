# Long signatures with generics and `where`

## As is (Rust)

```rust
fn run<T, E>(x: T) -> Result<T, E>
where
    T: Clone + Debug + Send,
    E: Error,
```

## Desired (sugar)

```rust
fn run(x: T: Clone + Debug + Send) -> Result<T, E: Error>
```

## Implementation

Attribute macro that lifts inline bounds into the generic list and a `where` clause.
