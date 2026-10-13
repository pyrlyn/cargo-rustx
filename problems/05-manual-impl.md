# Ручной `impl` для простых случаев

## Как есть (Rust)

```rust
impl From<io::Error> for MyErr {
    fn from(e: io::Error) -> Self { MyErr::Io(e) }
}
```

## Как хочется

```rust
enum MyErr { Io(from io::Error) }
```

## Реализация

Derive/атрибутный макрос генерирует `From`, `Display` и конструкторы (как thiserror / derive_more).
