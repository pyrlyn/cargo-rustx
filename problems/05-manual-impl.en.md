# Manual `impl` for simple cases

## As is (Rust)

```rust
impl From<io::Error> for MyErr {
    fn from(e: io::Error) -> Self { MyErr::Io(e) }
}
```

## Desired (sugar)

```rust
enum MyErr { Io(from io::Error) }
```

## Implementation

Derive/attribute macro generating `From`, `Display` and constructors (like thiserror / derive_more).

## Detailed example

### As is (valid Rust)

```rust
use std::fmt;
use std::io;
use std::num::ParseIntError;

#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    Parse(ParseIntError),
    Missing(String),
}

impl From<io::Error> for LoadError {
    fn from(e: io::Error) -> Self {
        LoadError::Io(e)
    }
}

impl From<ParseIntError> for LoadError {
    fn from(e: ParseIntError) -> Self {
        LoadError::Parse(e)
    }
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "io error: {}", e),
            LoadError::Parse(e) => write!(f, "parse error: {}", e),
            LoadError::Missing(k) => write!(f, "missing key: {}", k),
        }
    }
}

impl std::error::Error for LoadError {}

pub fn load_port(path: &str) -> Result<u16, LoadError> {
    let text = std::fs::read_to_string(path)?;
    let line = text
        .lines()
        .find(|l| l.starts_with("port="))
        .ok_or_else(|| LoadError::Missing("port".into()))?;
    Ok(line["port=".len()..].trim().parse()?)
}
```

### Desired (cargo-rustx)

```rust
// Function-like entry point: the body is not valid Rust, so #[sugar] on an item cannot be used
sugar! {
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
}
```

### What changed

- Two hand-written `impl From` became a `from` marker on the enum variant.
- `impl Display` and `impl Error` are generated from `#[display(..)]` attributes.
- ~45 lines shrank to ~20; `load_port` logic is unchanged.
