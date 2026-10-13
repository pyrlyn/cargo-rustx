# Turbofish

## As is (Rust)

```rust
let n = s.parse::<u32>()?;
let v = it.collect::<Vec<_>>();
```

## Desired (sugar)

```rust
let n = s.parse<u32>()?;
let v = it.collect<Vec>();
```

## Implementation

Token rewrite of `.method<T>(` into `.method::<T>(`, filling missing `<_>` params.
