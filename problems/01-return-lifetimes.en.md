# Lifetimes in return types

## As is (Rust)

```rust
fn first<'a>(xs: &'a [Item], key: &str) -> &'a Item
```

## Desired (sugar)

```rust
fn first(xs: &[Item], key: &str) -> &from(xs) Item
```

## Implementation

Attribute macro that introduces a named lifetime and binds it to the parameter referenced by `from(..)`.

## Detailed example

### As is (valid Rust)

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

### Desired (cargo-rustx)

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

### What changed

- `<'a>` / `<'a, 'b>` and every `&'a` are gone; the borrow source is named with `&from(x) T`.
- The macro introduces the lifetime parameter and ties it to the argument in `from(..)`.
- Also: `var` instead of `let mut`, `#[data]` on the struct.
