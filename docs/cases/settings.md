# Service settings

A settings struct with defaults, a listen address that usually keeps the default port, and a line parser that fails on an unknown key. In ordinary Rust that is a `Default` impl, `Option` parameters, `impl Into` plus a conversion, and a `Result` signature. [Field defaults](../guide/field-defaults.md), [`into`](../guide/into-params.md), [parameter defaults](../guide/param-defaults.md), [`throws`](../guide/throws.md) and [`var`](../guide/var.md) carry it.

## Plain Rust

```rust
#[derive(Debug, PartialEq)]
struct Settings {
    host: String,
    port: u16,
    user: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            host: "localhost".to_string(),
            port: 8080,
            user: "guest".to_string(),
        }
    }
}

fn listen(host: impl Into<String>, port: Option<u16>) -> String {
    let host = host.into();
    let port = port.unwrap_or(8080);
    format!("{host}:{port}")
}

fn load(text: &str) -> rustx::anyhow::Result<Settings> {
    let mut settings = Settings::default();
    for line in text.lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match key.trim() {
            "host" => settings.host = value.trim().into(),
            "port" => settings.port = value.trim().parse()?,
            "user" => settings.user = value.trim().into(),
            _ => return Err(rustx::anyhow::anyhow!("unknown key")),
        }
    }
    Ok(settings)
}

fn demo(text: &str) -> (String, u16, String, String, String) {
    let settings = load(text).unwrap();
    (
        settings.host,
        settings.port,
        settings.user,
        listen("db", None),
        listen("db", Some(9)),
    )
}

let (host, port, user, default_addr, named_addr) = demo("host=db\nport=9\n");
assert_eq!((host.as_str(), port, user.as_str()), ("db", 9, "guest"));
assert_eq!((default_addr.as_str(), named_addr.as_str()), ("db:8080", "db:9"));
assert_eq!(load("mode=dev").unwrap_err().to_string(), "unknown key");
assert!(load("port=nope").is_err());
```

## With rustx

```rust
rustx::sugar! {
    #[derive(Debug, PartialEq)]
    struct Settings {
        host: String = "localhost",
        port: u16 = 8080,
        user: String = "guest",
    }

    fn listen(host: into String, port: u16 = 8080) -> String {
        format!("{host}:{port}")
    }

    fn load(text: &str) -> Settings throws {
        var settings = Settings { .. };
        for line in text.lines() {
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            match key.trim() {
                "host" => settings.host = value.trim().into(),
                "port" => settings.port = value.trim().parse()?,
                "user" => settings.user = value.trim().into(),
                _ => throw "unknown key",
            }
        }
        settings
    }

    fn demo(text: &str) -> (String, u16, String, String, String) {
        let settings = load(text).unwrap();
        (
            settings.host,
            settings.port,
            settings.user,
            listen("db"),
            listen("db", port: 9),
        )
    }

    fn unknown(text: &str) -> String {
        load(text).unwrap_err().to_string()
    }

    fn bad_port(text: &str) -> bool {
        load(text).is_err()
    }
}

let (host, port, user, default_addr, named_addr) = demo("host=db\nport=9\n");
assert_eq!((host.as_str(), port, user.as_str()), ("db", 9, "guest"));
assert_eq!((default_addr.as_str(), named_addr.as_str()), ("db:8080", "db:9"));
assert_eq!(unknown("mode=dev"), "unknown key");
assert!(bad_port("port=nope"));
```
