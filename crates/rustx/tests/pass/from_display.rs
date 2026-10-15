// Problem 05: `from` variants and `#[display]` through `sugar!`.
rustx::sugar! {
    mod load {
        use std::io;
        use std::num::ParseIntError;

        #[derive(Debug)]
        pub enum LoadError {
            #[display("io error: {0}")]
            Io(from io::Error),
            #[display("parse error: {0}")]
            Parse(from ParseIntError),
            #[display("missing key: {0}")]
            Missing(String),
        }

        pub fn load_port(path: &str) -> Result<u16, LoadError> {
            let text = std::fs::read_to_string(path)?;
            let line = text
                .lines()
                .find(|l| l.starts_with("port="))
                .ok_or_else(|| LoadError::Missing("port".into()))?;
            Ok(line["port=".len()..].trim().parse()?)
        }
    }

    // Generated impls add no bounds, so the enum states what `#[display]` and `Error` need.
    #[derive(Debug, PartialEq)]
    enum Wrapped<T: std::fmt::Display + std::fmt::Debug> {
        #[display("wrapped {0:>4}")]
        Value(from T),
    }

    #[derive(Debug)]
    enum Event {
        #[display("key {code:#x} ({name})")]
        Key { name: &'static str, code: u32 },
        #[display("{} then {}")]
        Pair(u8, char),
        #[display("char {0:?}")]
        Char(from char),
        #[display("quit")]
        Quit,
    }
}

use load::{LoadError, load_port};

fn main() {
    let dir = std::env::temp_dir().join(format!("rustx-from-display-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = |name: &str, text: &str| {
        let path = dir.join(name);
        std::fs::write(&path, text).unwrap();
        path.to_str().unwrap().to_owned()
    };

    assert_eq!(load_port(&file("ok", "host=a\nport= 8080\n")).unwrap(), 8080);

    // `?` converts each error through the generated `From`.
    let parse = load_port(&file("bad", "port=eighty\n")).unwrap_err();
    assert!(matches!(parse, LoadError::Parse(_)));
    assert_eq!(parse.to_string(), "parse error: invalid digit found in string");

    let missing = load_port(&file("none", "host=a\n")).unwrap_err();
    assert_eq!(missing.to_string(), "missing key: port");

    let io = load_port(dir.join("absent").to_str().unwrap()).unwrap_err();
    assert!(matches!(&io, LoadError::Io(e) if e.kind() == std::io::ErrorKind::NotFound));
    assert!(io.to_string().starts_with("io error: "));

    // The generated `Error` impl makes the enum a boxed error.
    let boxed: Box<dyn std::error::Error> = Box::new(missing);
    assert_eq!(boxed.to_string(), "missing key: port");

    assert_eq!(Wrapped::from(7), Wrapped::Value(7));
    assert_eq!(Wrapped::from("ab").to_string(), "wrapped   ab");

    assert_eq!(Event::Pair(0, 'x').to_string(), "0 then x");
    assert_eq!(Event::from('y').to_string(), "char 'y'");
    let key = Event::Key { name: "enter", code: 13 };
    assert_eq!(key.to_string(), "key 0xd (enter)");
    assert_eq!(Event::Quit.to_string(), "quit");

    std::fs::remove_dir_all(&dir).unwrap();
}
