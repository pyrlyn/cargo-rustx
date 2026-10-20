//! Typed ids, an email address and a length, written without sugar.

use std::fmt;

/// A user id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UserId(u64);

#[allow(dead_code)]
impl UserId {
    pub fn new(__rustx_inner: u64) -> Self {
        Self(__rustx_inner)
    }
}

impl fmt::Display for UserId {
    fn fmt(&self, __rustx_f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(&self.0, __rustx_f)
    }
}

impl ::core::str::FromStr for UserId {
    type Err = <u64 as ::core::str::FromStr>::Err;
    fn from_str(__rustx_s: &str) -> ::core::result::Result<Self, Self::Err> {
        <u64 as ::core::str::FromStr>::from_str(__rustx_s).map(Self)
    }
}

/// An address with an `@`.
#[derive(Debug)]
pub struct Email(String);

#[allow(dead_code)]
impl Email {
    pub fn new(__rustx_inner: String) -> ::core::result::Result<Self, &'static str> {
        check_email(&__rustx_inner)?;
        ::core::result::Result::Ok(Self(__rustx_inner))
    }
}

impl ::core::ops::Deref for Email {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl ::core::str::FromStr for Email {
    type Err = &'static str;
    fn from_str(__rustx_s: &str) -> ::core::result::Result<Self, Self::Err> {
        Self::new(::core::convert::From::from(__rustx_s))
    }
}

fn check_email(s: &String) -> Result<(), &'static str> {
    if s.contains('@') { Ok(()) } else { Err("no @") }
}

/// A length.
pub struct Meters(f64);

#[allow(dead_code)]
impl Meters {
    pub fn new(__rustx_inner: f64) -> Self {
        Self(__rustx_inner)
    }
}

impl ::core::ops::Add for Meters {
    type Output = Self;
    fn add(self, __rustx_other: Self) -> Self {
        Self(::core::ops::Add::add(self.0, __rustx_other.0))
    }
}

// Rounded, which no `display` flag writes, so this impl stays.
impl fmt::Display for Meters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:.1} m", self.0)
    }
}
