# `impl Trait` и сложные возвращаемые итераторы

## Как есть (Rust)

```rust
fn evens(v: &[i32]) -> impl Iterator<Item = &i32> + '_
```

## Как хочется

```rust
fn evens(v: &[i32]) -> iter &i32
```

## Реализация

Десугаринг `iter T` в `impl Iterator<Item = T> + '_`.

## Подробный пример

### Как есть (валидный Rust)

```rust
#[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
pub enum Level {
    Debug,
    Info,
    Warn,
    Error,
}

pub struct Entry {
    pub level: Level,
    pub module: String,
    pub msg: String,
}

pub struct Log {
    entries: Vec<Entry>,
}

impl Log {
    pub fn at_least(&self, min: Level) -> impl Iterator<Item = &Entry> + '_ {
        self.entries.iter().filter(move |e| e.level >= min)
    }

    pub fn modules(&self) -> impl Iterator<Item = &str> + '_ {
        self.entries.iter().map(|e| e.module.as_str())
    }

    pub fn grep<'a>(&'a self, needle: &'a str) -> impl Iterator<Item = &'a Entry> + 'a {
        self.entries.iter().filter(move |e| e.msg.contains(needle))
    }
}

pub fn numbered<'a, I>(it: I) -> impl Iterator<Item = (usize, &'a Entry)>
where
    I: Iterator<Item = &'a Entry>,
{
    it.enumerate().map(|(i, e)| (i + 1, e))
}

pub fn errors_in(log: &Log, module: &str) -> usize {
    log.at_least(Level::Error).filter(|e| e.module == module).count()
}
```

### Как хочется (BetterDiffuseRust)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugar] on an item cannot be used
sugar! {
mod log {
    #[derive(Debug, Clone, Copy, PartialEq, PartialOrd)]
    pub enum Level { Debug, Info, Warn, Error }

    #[data]
    pub struct Entry { level: Level, module: String, msg: String }

    pub struct Log { entries: Vec<Entry> }

    impl Log {
        pub fn at_least(&self, min: Level) -> iter &Entry {
            self.entries.iter().filter(move |e| e.level >= min)
        }

        pub fn modules(&self) -> iter &str {
            self.entries.iter().map(|e| e.module.as_str())
        }

        pub fn grep(&self, needle: &str) -> iter &from(self, needle) Entry {
            self.entries.iter().filter(move |e| e.msg.contains(needle))
        }
    }

    pub fn numbered(it: I: Iterator<Item = &Entry>) -> iter (usize, &from(it) Entry) {
        it.enumerate().map(|(i, e)| (i + 1, e))
    }

    pub fn errors_in(log: &Log, module: &str) -> usize {
        log.at_least(Level::Error).filter(|e| e.module == module).count()
    }
}
}
```

### Что изменилось

- `impl Iterator<Item = T> + '_` заменён на `iter T`.
- Лайфтаймы для нескольких источников задаются через `&from(a, b) T`.
- Попутно: inline-ограничение `I: Iterator<..>` вместо `where`, `#[data]`.
