# Default values: `..Default::default()` and `Option` parameters

## As is (Rust)

```rust
impl Default for Config { fn default() -> Self { Config { port: 8080, workers: 4 } } }
let c = Config { port: 3000, ..Default::default() };
fn connect(host: &str, port: Option<u16>) { let port = port.unwrap_or(5432); }
```

## Desired (sugar)

```rust
struct Config { port: u16 = 8080, workers: usize = 4 }
let c = Config { port: 3000, .. };
fn connect(host: &str, port: u16 = 5432) { }
```

## Implementation

Field defaults (`field: T = expr`) generate `impl Default` and turn `Struct { .., }` into `Struct { .., ..Default::default() }`. The syntax follows RFC 3681 (default field values), which is still unstable upstream; once it is stable the pass can emit the native form. Parameter defaults (`p: T = expr`) become `p: Option<T>` plus `let p = p.unwrap_or(expr);` (`unwrap_or_else` for non-constant expressions). Calls written in the same `sugar!` input get missing trailing arguments filled with `None` and given ones wrapped in `Some(..)`. Named arguments (`port: 6432`) are allowed only for defaulted parameters.

## Detailed example

### As is (valid Rust)

```rust
#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: usize,
    pub timeout_ms: u64,
    pub tls: bool,
}

impl Default for ServerConfig {
    fn default() -> Self {
        ServerConfig {
            host: "127.0.0.1".to_string(),
            port: 8080,
            workers: 4,
            timeout_ms: 30_000,
            tls: false,
        }
    }
}

pub fn dev() -> ServerConfig {
    ServerConfig { port: 3000, ..Default::default() }
}

pub fn prod(host: &str) -> ServerConfig {
    ServerConfig { host: host.to_string(), workers: 16, tls: true, ..ServerConfig::default() }
}

pub fn connect(host: &str, port: Option<u16>, timeout_ms: Option<u64>) -> String {
    let port = port.unwrap_or(5432);
    let timeout_ms = timeout_ms.unwrap_or(5_000);
    format!("{host}:{port} ({timeout_ms} ms)")
}

pub fn demo() -> Vec<String> {
    vec![
        connect("db", None, None),
        connect("db", Some(6432), None),
        connect("db", None, Some(100)),
    ]
}
```

### Desired (cargo-rustx)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugar] on an item cannot be used
sugar! {
mod server {
    #[derive(Debug, Clone)]
    pub struct ServerConfig {
        pub host: String = "127.0.0.1",
        pub port: u16 = 8080,
        pub workers: usize = 4,
        pub timeout_ms: u64 = 30_000,
        pub tls: bool = false,
    }

    pub fn dev() -> ServerConfig {
        ServerConfig { port: 3000, .. }
    }

    pub fn prod(host: &str) -> ServerConfig {
        ServerConfig { host: host.to_string(), workers: 16, tls: true, .. }
    }

    pub fn connect(host: &str, port: u16 = 5432, timeout_ms: u64 = 5_000) -> String {
        format!("{host}:{port} ({timeout_ms} ms)")
    }

    pub fn demo() -> Vec<String> {
        vec![
            connect("db"),
            connect("db", port: 6432),
            connect("db", timeout_ms: 100),
        ]
    }
}
}
```

### What changed

- The hand-written `impl Default` became `= value` next to each field. The default sits where the field is declared.
- `..Default::default()` and `..ServerConfig::default()` became `..`, the spelling from RFC 3681.
- `Option<T>` parameters with `unwrap_or` in the body became `p: T = value`. Inside the same `sugar!`, calls skip defaulted arguments or name them.
- Callers outside the macro see the real signature, `connect(host, Option<u16>, Option<u64>)`, because the macro cannot rewrite code it does not see.
- Also: the string literal default for a `String` field (item 15).
