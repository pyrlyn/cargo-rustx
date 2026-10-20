//! Load errors with the impls `from` and `#[display]` generate, written without sugar.

use std::io;
use std::num::ParseIntError;

/// Why the port could not be read.
#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    Parse(ParseIntError),
    // The key is quoted back, so a typo shows up in the message.
    Missing(String),
}

impl ::core::convert::From<io::Error> for LoadError {
    fn from(__rustx_value: io::Error) -> Self {
        Self::Io(__rustx_value)
    }
}

impl ::core::convert::From<ParseIntError> for LoadError {
    fn from(__rustx_value: ParseIntError) -> Self {
        Self::Parse(__rustx_value)
    }
}

impl ::core::fmt::Display for LoadError {
    fn fmt(&self, __rustx_f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        match self {
            Self::Io(__rustx_arg0) => ::core::write!(__rustx_f, "io: {0}", __rustx_arg0),
            Self::Parse(__rustx_arg0) => ::core::write!(__rustx_f, "number: {0}", __rustx_arg0),
            Self::Missing(__rustx_arg0) => ::core::write!(__rustx_f, "missing {0}", __rustx_arg0),
        }
    }
}

impl ::std::error::Error for LoadError {}

pub fn load_port(path: &str) -> Result<u16, LoadError> {
    let text = std::fs::read_to_string(path)?;
    let line = text
        .lines()
        .find(|l| l.starts_with("port="))
        .ok_or_else(|| LoadError::Missing("port".into()))?;
    Ok(line["port=".len()..].trim().parse()?)
}

/// What a write failed on; its `Display` is the one rustfmt keeps in a block per arm.
#[derive(Debug)]
pub enum SaveError {
    Io(io::Error),
}

impl ::core::convert::From<io::Error> for SaveError {
    fn from(__rustx_value: io::Error) -> Self {
        Self::Io(__rustx_value)
    }
}

impl ::core::fmt::Display for SaveError {
    fn fmt(&self, __rustx_f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
        match self {
            Self::Io(__rustx_arg0) => {
                ::core::write!(__rustx_f, "could not write the file: {0}", __rustx_arg0)
            }
        }
    }
}
