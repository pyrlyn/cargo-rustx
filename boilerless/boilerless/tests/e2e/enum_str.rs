//! E2E `enum_str`: log level <-> string.
//! Manual boilerplate: `as_str` match, `Display`, `FromStr` match with an error arm.
//! Macro solution: `#[boil(enum_str)]` with `rename`.
//! Checks: round-trip of every variant and identical error behaviour.

use boilerless::Boil;
use std::{fmt, str::FromStr};

// ---- manual boilerplate ----
#[derive(Debug, PartialEq, Clone, Copy)]
enum ManualLevel { Warn, Error }
impl ManualLevel {
    fn as_str(&self) -> &'static str {
        match self { Self::Warn => "warn", Self::Error => "Error" }
    }
}
impl fmt::Display for ManualLevel {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result { f.write_str(self.as_str()) }
}
impl FromStr for ManualLevel {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, String> {
        match s { "warn" => Ok(Self::Warn), "Error" => Ok(Self::Error), o => Err(format!("unknown level {o}")) }
    }
}

// ---- macro solution ----
#[derive(Boil, Debug, PartialEq, Clone, Copy)]
#[boil(enum_str)]
enum Level {
    #[boil(rename = "warn")]
    Warn,
    Error,
}

fn main() {
    for (m, g) in [(ManualLevel::Warn, Level::Warn), (ManualLevel::Error, Level::Error)] {
        assert_eq!(m.to_string(), g.to_string());
        assert_eq!(m.as_str(), g.as_str());
        assert_eq!(g.as_str().parse::<Level>(), Ok(g));
    }
    assert!("nope".parse::<ManualLevel>().is_err());
    assert!("nope".parse::<Level>().is_err());
}
