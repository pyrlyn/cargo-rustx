# Constructors and getters

## As is (Rust)

```rust
impl User {
    pub fn new(name: String, age: u32) -> Self { Self { name, age } }
    pub fn name(&self) -> &str { &self.name }
}
```

## Desired (sugar)

```rust
#[data]
struct User { name: String, age: u32 }
```

## Implementation

Derive macro generating `new`, getters and an optional builder.
