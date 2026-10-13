# Manual `impl` for simple cases

## As is (Rust)

```rust
impl From<io::Error> for MyErr {
    fn from(e: io::Error) -> Self { MyErr::Io(e) }
}
```

## Desired (sugar)

```rust
enum MyErr { Io(from io::Error) }
```

## Implementation

Derive/attribute macro generating `From`, `Display` and constructors (like thiserror / derive_more).
