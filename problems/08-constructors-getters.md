# Конструкторы и геттеры

## Как есть (Rust)

```rust
impl User {
    pub fn new(name: String, age: u32) -> Self { Self { name, age } }
    pub fn name(&self) -> &str { &self.name }
}
```

## Как хочется

```rust
#[data]
struct User { name: String, age: u32 }
```

## Реализация

Derive-макрос генерирует `new`, геттеры и опциональный builder.
