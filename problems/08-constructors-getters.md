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

## Подробный пример

### Как есть (валидный Rust)

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct User {
    id: u64,
    name: String,
    email: String,
    active: bool,
}

impl User {
    pub fn new(id: u64, name: String, email: String) -> Self {
        User { id, name, email, active: true }
    }

    pub fn id(&self) -> u64 {
        self.id
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn email(&self) -> &str {
        &self.email
    }

    pub fn active(&self) -> bool {
        self.active
    }

    pub fn set_active(&mut self, active: bool) {
        self.active = active;
    }

    pub fn with_email(mut self, email: String) -> Self {
        self.email = email;
        self
    }
}

pub fn deactivate_by_domain(users: &mut [User], domain: &str) -> usize {
    let mut n = 0;
    for u in users.iter_mut() {
        if u.email().ends_with(domain) && u.active() {
            u.set_active(false);
            n += 1;
        }
    }
    n
}
```

### Как хочется (BetterDiffuseRust)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugar] on an item cannot be used
sugar! {
mod users {
    #[data(setters, with)]
    #[derive(Debug, Clone, PartialEq)]
    pub struct User {
        id: u64,
        name: String,
        email: String,
        #[default(true)]
        active: bool,
    }

    pub fn deactivate_by_domain(users: &mut [User], domain: &str) -> usize {
        var n = 0;
        for u in users.iter_mut() {
            if u.email().ends_with(domain) && u.active() {
                u.set_active(false);
                n += 1;
            }
        }
        n
    }
}
}
```

### Что изменилось

- Весь `impl User` (`new`, 4 геттера, сеттер, `with_*`) генерирует `#[data]`.
- `#[default(true)]` убирает поле из аргументов `new`.
- Геттеры для `String` возвращают `&str`, для `Copy`-типов — значение.
