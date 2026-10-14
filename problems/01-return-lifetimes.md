# Лайфтаймы в возвращаемом типе

## Как есть (Rust)

```rust
fn first<'a>(xs: &'a [Item], key: &str) -> &'a Item
```

## Как хочется

```rust
fn first(xs: &[Item], key: &str) -> &from(xs) Item
```

## Реализация

Атрибутный макрос вводит именованный лайфтайм и привязывает его к параметру из `from(..)`.

## Подробный пример

### Как есть (валидный Rust)

```rust
use std::collections::HashMap;

pub struct Config {
    entries: HashMap<String, String>,
}

impl Config {
    pub fn parse(text: &str) -> Config {
        let mut entries = HashMap::new();
        for line in text.lines() {
            if let Some((k, v)) = line.split_once('=') {
                entries.insert(k.trim().to_string(), v.trim().to_string());
            }
        }
        Config { entries }
    }

    pub fn get<'a>(&'a self, key: &str) -> Option<&'a str> {
        self.entries.get(key).map(|s| s.as_str())
    }
}

pub fn longest_value<'a>(cfg: &'a Config, keys: &[&str]) -> Option<&'a str> {
    let mut best: Option<&'a str> = None;
    for k in keys {
        if let Some(v) = cfg.get(k) {
            if best.map_or(true, |b| v.len() > b.len()) {
                best = Some(v);
            }
        }
    }
    best
}

pub fn section<'a, 'b>(name: &'a str, _sep: &'b str) -> &'a str {
    name.split('.').next().unwrap_or(name)
}
```

### Как хочется (cargo-rustx)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugar] on an item cannot be used
sugar! {
mod config {
    use std::collections::HashMap;

    #[data]
    pub struct Config {
        entries: HashMap<String, String>,
    }

    impl Config {
        pub fn parse(text: &str) -> Config {
            var entries = HashMap::new();
            for line in text.lines() {
                if let Some((k, v)) = line.split_once('=') {
                    entries.insert(k.trim().to_string(), v.trim().to_string());
                }
            }
            Config { entries }
        }

        pub fn get(&self, key: &str) -> Option<&from(self) str> {
            self.entries.get(key).map(|s| s.as_str())
        }
    }

    pub fn longest_value(cfg: &Config, keys: &[&str]) -> Option<&from(cfg) str> {
        var best = None;
        for k in keys {
            if let Some(v) = cfg.get(k) {
                if best.map_or(true, |b: &str| v.len() > b.len()) {
                    best = Some(v);
                }
            }
        }
        best
    }

    pub fn section(name: &str, _sep: &str) -> &from(name) str {
        name.split('.').next().unwrap_or(name)
    }
}
}
```

### Что изменилось

- `<'a>` / `<'a, 'b>` и все `&'a` убраны; источник заимствования указан через `&from(x) T`.
- Макрос сам вводит лайфтайм-параметр и связывает его с аргументом из `from(..)`.
- Попутно: `var` вместо `let mut`, `#[data]` на структуре.
