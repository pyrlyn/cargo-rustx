//! E2E `error`: an application error enum with messages, `Error` and `?`-friendly `From`.
//! Manual boilerplate: `impl Display` with a match, `impl Error`, one `impl From` per source.
//! Macro solution: `#[derive(Boil)] #[boil(error)]` with `msg`/`from` per variant.
//! Checks: both versions produce identical messages and `?` conversions.

use boilerless::Boil;
use std::fmt;

// ---- manual boilerplate ----
#[derive(Debug)]
enum ManualError {
    Missing { key: String },
    Parse(std::num::ParseIntError),
}
impl fmt::Display for ManualError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Missing { key } => write!(f, "missing key {key}"),
            Self::Parse(e) => write!(f, "parse failed: {e}"),
        }
    }
}
impl std::error::Error for ManualError {}
impl From<std::num::ParseIntError> for ManualError {
    fn from(e: std::num::ParseIntError) -> Self {
        Self::Parse(e)
    }
}

// ---- macro solution ----
#[derive(Boil, Debug)]
#[boil(error)]
enum MacroError {
    #[boil(msg = "missing key {key}")]
    Missing { key: String },
    #[boil(msg = "parse failed: {_0}", from)]
    Parse(std::num::ParseIntError),
}

fn manual(s: &str) -> Result<i64, ManualError> {
    if s.is_empty() {
        return Err(ManualError::Missing { key: "n".into() });
    }
    Ok(s.parse()?)
}
fn with_macro(s: &str) -> Result<i64, MacroError> {
    if s.is_empty() {
        return Err(MacroError::Missing { key: "n".into() });
    }
    Ok(s.parse()?)
}

fn main() {
    for input in ["", "x", "12"] {
        let a = manual(input).map_err(|e| e.to_string());
        let b = with_macro(input).map_err(|e| e.to_string());
        assert_eq!(a, b, "input {input:?}");
    }
    let boxed: Box<dyn std::error::Error> = Box::new(MacroError::Missing { key: "k".into() });
    assert_eq!(boxed.to_string(), "missing key k");
}
