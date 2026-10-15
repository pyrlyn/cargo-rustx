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
                // `anyhow::Error` takes no `String`, so the problem's `format!` becomes `anyhow!`.
                let (k, v) = line
                    .split_once('=')
                    .ok_or_else(|| ::rustx::anyhow::anyhow!("line {}: expected key=value", n + 1))?;
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
            // `throw` is T21; until then the early exit is spelled out.
            if workers == 0 { return Err(::rustx::anyhow::anyhow!("workers must be > 0")); }
            Settings::new(host, port, workers)
        }
    }

    mod own_error {
        #[derive(Debug, PartialEq)]
        pub enum ParseError { Empty, Bad }

        pub fn parse(text: &str) -> u32 throws ParseError {
            if text.is_empty() { return Err(ParseError::Empty); }
            text.parse().map_err(|_| ParseError::Bad)?
        }

        pub fn checked(text: &str) -> u32 throws ParseError {
            let n = parse(text)?;
            if n == 0 { return Err(ParseError::Empty); }
            Ok(n)
        }

        pub trait Source {
            fn text(&self) -> String throws ParseError;
            fn number(&self) -> u32 throws ParseError { parse(&self.text()?)? }
            fn check(&self) -> () throws ParseError { self.number()?; }
        }

        pub struct Fixed(pub &'static str);

        impl Source for Fixed {
            fn text(&self) -> String throws ParseError { self.0.to_string() }
        }

        impl Fixed {
            pub fn words(&self) -> iter &str throws { self.0.split(' ') }
            pub fn first(&self) -> String throws {
                let mut words = self.words()?;
                words.next().map(str::to_string).ok_or_else(|| ::rustx::anyhow::anyhow!("empty"))?
            }
        }
    }
}

use own_error::{Fixed, ParseError, Source};

fn main() {
    let path = std::env::temp_dir().join(format!("rustx-throws-{}.conf", std::process::id()));
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
    assert!(settings::load("/nonexistent/rustx.conf").is_err());

    assert_eq!(own_error::parse("42"), Ok(42));
    assert_eq!(own_error::parse(""), Err(ParseError::Empty));
    assert_eq!(own_error::parse("x"), Err(ParseError::Bad));
    assert_eq!(own_error::checked("0"), Err(ParseError::Empty));
    assert_eq!(own_error::checked("7"), Ok(7));

    assert_eq!(Fixed("12").number(), Ok(12));
    assert_eq!(Fixed("12").check(), Ok(()));
    assert_eq!(Fixed("").check(), Err(ParseError::Empty));
    assert_eq!(Fixed("a b").words().unwrap().collect::<Vec<_>>(), ["a", "b"]);
    assert_eq!(Fixed("a b").first().unwrap(), "a");
}
