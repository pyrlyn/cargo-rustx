# Boilerplate вокруг трейтов: делегирование

## Как есть (Rust)

```rust
impl Read for Wrapper {
    fn read(&mut self, b: &mut [u8]) -> io::Result<usize> {
        self.inner.read(b)
    }
}
```

## Как хочется

```rust
struct Wrapper {
    #[delegate(Read)]
    inner: File,
}
```

## Реализация

Атрибутный макрос делегирования (см. крейты `delegate`, `ambassador`).
