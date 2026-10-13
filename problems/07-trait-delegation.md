# Trait boilerplate: delegation

## As is (Rust)

```rust
impl Read for Wrapper {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.inner.read(b)
    }
}
```

## Desired (sugar)

```rust
struct Wrapper {
    #[delegate(Read)]
    inner: File,
}
```

## Implementation

Delegation attribute macro (see `delegate`, `ambassador` crates).
