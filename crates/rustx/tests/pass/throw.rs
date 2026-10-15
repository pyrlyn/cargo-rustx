rustx::sugar! {
    mod settings {
        use std::collections::HashMap;
        use std::fs;

        #[derive(Debug)]
        #[data]
        pub struct Settings { host: String, port: u16, workers: usize }

        fn parse_kv(text: &str) -> HashMap<String, String> throws {
            var map = HashMap::new();
            for (n, line) in text.lines().enumerate() {
                let line = line.trim();
                if line.is_empty() || line.starts_with('#') { continue; }
                // `anyhow::Error` takes no `String` through `?`, so the problem's
                // `ok_or_else(|| format!(..))?` throws the message instead.
                let Some((k, v)) = line.split_once('=') else {
                    throw format!("line {}: expected key=value", n + 1)
                };
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

    mod errors {
        use std::num::ParseIntError;

        #[derive(Debug, PartialEq)]
        pub enum ConfigError { Missing(&'static str), Zero, Number(ParseIntError) }

        impl std::fmt::Display for ConfigError {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{self:?}")
            }
        }

        impl std::error::Error for ConfigError {}

        impl From<ParseIntError> for ConfigError {
            fn from(err: ParseIntError) -> Self { ConfigError::Number(err) }
        }

        // An error value under plain `throws` stays the source, so callers can downcast it.
        pub fn number(text: &str) -> u32 throws {
            match text.parse<u32>() {
                Ok(n) => n,
                Err(err) => throw err,
            }
        }

        // A custom error enum under plain `throws`.
        pub fn positive(n: u32) -> u32 throws {
            if n == 0 { throw ConfigError::Zero }
            n
        }

        // `throws E`: the enum itself, and a value converted into it through `From`.
        pub fn require(value: Option<&str>, name: &'static str) -> u32 throws ConfigError {
            let text = match value { Some(text) => text, None => throw ConfigError::Missing(name), };
            let n = match text.parse<u32>() { Ok(n) => n, Err(err) => throw err, };
            fn check(n: u32) -> u32 throws ConfigError { if n == 0 { throw ConfigError::Zero } n }
            let doubled = check(n).map(|n| n * 2);
            doubled?
        }

        // A string into a boxed error.
        pub fn first_line(text: &str) -> &str throws Box<dyn std::error::Error + Send + Sync> {
            let Some(line) = text.lines().next() else { throw "empty input" };
            line
        }
    }
}

use errors::ConfigError;

fn main() {
    let path = std::env::temp_dir().join(format!("rustx-throw-{}.conf", std::process::id()));
    std::fs::write(&path, "# demo\nhost = example.org\nworkers = 2\n").unwrap();
    let loaded = settings::load(path.to_str().unwrap());
    std::fs::write(&path, "workers = 0\n").unwrap();
    let zero = settings::load(path.to_str().unwrap());
    std::fs::write(&path, "no equals sign\n").unwrap();
    let malformed = settings::load(path.to_str().unwrap());
    std::fs::remove_file(&path).unwrap();

    let loaded = loaded.unwrap();
    assert_eq!((loaded.host(), loaded.port(), loaded.workers()), ("example.org", 8080, 2));
    assert_eq!(zero.unwrap_err().to_string(), "workers must be > 0");
    assert_eq!(malformed.unwrap_err().to_string(), "line 1: expected key=value");

    assert_eq!(errors::number("5").unwrap(), 5);
    let bad = errors::number("x").unwrap_err();
    assert!(bad.downcast_ref::<std::num::ParseIntError>().is_some());
    let zero = errors::positive(0).unwrap_err();
    assert_eq!(zero.downcast_ref::<ConfigError>(), Some(&ConfigError::Zero));
    assert_eq!(errors::positive(3).unwrap(), 3);

    assert_eq!(errors::require(Some("4"), "n"), Ok(8));
    assert_eq!(errors::require(None, "n"), Err(ConfigError::Missing("n")));
    assert_eq!(errors::require(Some("0"), "n"), Err(ConfigError::Zero));
    assert!(matches!(errors::require(Some("x"), "n"), Err(ConfigError::Number(_))));

    assert_eq!(errors::first_line("a\nb").unwrap(), "a");
    assert_eq!(errors::first_line("").unwrap_err().to_string(), "empty input");
}
