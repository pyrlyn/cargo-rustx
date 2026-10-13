# Ошибки и `Result`

## Как есть (Rust)

```rust
fn load() -> Result<Config, Box<dyn std::error::Error + Send + Sync>>
```

## Как хочется

```rust
fn load() -> Config throws
```

## Реализация

Атрибутный макрос оборачивает тип в `anyhow::Result<T>`, а хвостовое выражение в `Ok(..)`.

## Подробный пример

### Как есть (валидный Rust)

```rust
use std::collections::HashMap;
use std::error::Error;
use std::fs;

pub struct Settings {
    pub host: String,
    pub port: u16,
    pub workers: usize,
}

fn parse_kv(text: &str) -> Result<HashMap<String, String>, Box<dyn Error + Send + Sync>> {
    let mut map = HashMap::new();
    for (n, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let (k, v) = line
            .split_once('=')
            .ok_or_else(|| format!("line {}: expected key=value", n + 1))?;
        map.insert(k.trim().to_string(), v.trim().to_string());
    }
    Ok(map)
}

pub fn load(path: &str) -> Result<Settings, Box<dyn Error + Send + Sync>> {
    let text = fs::read_to_string(path)?;
    let kv = parse_kv(&text)?;
    let host = kv.get("host").cloned().unwrap_or_else(|| "localhost".into());
    let port = kv.get("port").map(|p| p.parse::<u16>()).transpose()?.unwrap_or(8080);
    let workers = match kv.get("workers") {
        Some(w) => w.parse::<usize>()?,
        None => 4,
    };
    if workers == 0 {
        return Err("workers must be > 0".into());
    }
    Ok(Settings { host, port, workers })
}
```

### Как хочется (BetterDiffuseRust)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugar] on an item cannot be used
sugar! {
mod settings {
    use std::collections::HashMap;
    use std::fs;

    #[data]
    pub struct Settings { host: String, port: u16, workers: usize }

    fn parse_kv(text: &str) -> HashMap<String, String> throws {
        var map = HashMap::new();
        for (n, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') { continue; }
            let (k, v) = line
                .split_once('=')
                .ok_or_else(|| format!("line {}: expected key=value", n + 1))?;
            map.insert(k.trim().to_string(), v.trim().to_string());
        }
        map
    }

    pub fn load(path: &str) -> Settings throws {
        let text = fs::read_to_string(path)?;
        let kv = parse_kv(&text)?;
        let host = kv.get("host").cloned().unwrap_or_else(|| "localhost".into());
        let port = kv.get("port").map(|p| p.parse<u16>()).transpose()?.unwrap_or(8080);
        let workers = match kv.get("workers") {
            Some(w) => w.parse<usize>()?,
            None => 4,
        };
        if workers == 0 { throw "workers must be > 0"; }
        Settings::new(host, port, workers)
    }
}
}
```

### Что изменилось

- `Result<T, Box<dyn Error + Send + Sync>>` заменён на `T throws` (десугаринг в `anyhow::Result<T>`).
- Хвостовое выражение оборачивается в `Ok(..)` автоматически; `throw x` → `return Err(x.into())`.
- Попутно: `.parse<u16>()` без turbofish, `var`, `#[data]`.
