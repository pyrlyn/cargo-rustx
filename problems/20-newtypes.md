# Newtype boilerplate

## As is (Rust)

```rust
pub struct Email(String);
impl Deref for Email { type Target = str; fn deref(&self) -> &str { &self.0 } }
impl fmt::Display for Email { .. }
impl FromStr for Email { .. }
```

## Desired (sugar)

```rust
#[newtype(deref, display, from_str, validate = check_email)]
pub struct Email(String);
```

## Implementation

Attribute macro on a one-field tuple struct, usable through `#[sugar]` because the input is valid Rust. Each flag generates one `impl`: `deref` (`Deref<Target = ..>`, `str` for `String`, `[T]` for `Vec<T>`), `as_ref`, `display` (optionally with a format string), `from_str`, `into` (`From<Newtype> for Inner`), and arithmetic operators (`add`, `sub`, `mul`, ...). With `validate = f`, where `f: fn(&Inner) -> Result<(), E>`, it also generates `new(inner) -> Result<Self, E>`, and `from_str` goes through it. Without `validate`, `new` is infallible.

## Detailed example

### As is (valid Rust)

```rust
use std::fmt;
use std::ops::{Add, Deref};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Email(String);

fn check_email(s: &String) -> Result<(), String> {
    if s.contains('@') { Ok(()) } else { Err(format!("invalid email: {s}")) }
}

impl Email {
    pub fn new(s: String) -> Result<Email, String> {
        check_email(&s)?;
        Ok(Email(s))
    }
}

impl Deref for Email {
    type Target = str;
    fn deref(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for Email {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Email {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for Email {
    type Err = String;
    fn from_str(s: &str) -> Result<Email, String> {
        Email::new(s.to_string())
    }
}

impl From<Email> for String {
    fn from(e: Email) -> String {
        e.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
pub struct Meters(pub f64);

impl Add for Meters {
    type Output = Meters;
    fn add(self, o: Meters) -> Meters {
        Meters(self.0 + o.0)
    }
}

impl fmt::Display for Meters {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} m", self.0)
    }
}

pub fn demo() -> Result<String, String> {
    let e: Email = "ann@example.com".parse()?;
    let total = Meters(1.5) + Meters(2.0);
    let domain = e.split_once('@').map(|(_, d)| d).unwrap_or("");
    Ok(format!("{e} ({domain}, {} chars), {total}", e.len()))
}
```

### Desired (cargo-rustx)

```rust
// Attribute entry point is enough: the input is valid Rust
#[sugar]
mod units {
    #[newtype(deref, as_ref, display, from_str, into, validate = check_email)]
    #[derive(Debug, Clone, PartialEq, Eq, Hash)]
    pub struct Email(String);

    fn check_email(s: &String) -> Result<(), String> {
        if s.contains('@') { Ok(()) } else { Err(format!("invalid email: {s}")) }
    }

    #[newtype(add, display = "{} m")]
    #[derive(Debug, Clone, Copy, PartialEq, PartialOrd, Default)]
    pub struct Meters(pub f64);

    pub fn demo() -> Result<String, String> {
        let e: Email = "ann@example.com".parse()?;
        let total = Meters(1.5) + Meters(2.0);
        let domain = e.split_once('@').map(|(_, d)| d).unwrap_or("");
        Ok(format!("{e} ({domain}, {} chars), {total}", e.len()))
    }
}
```

### What changed

- Seven hand-written `impl` blocks (about 55 lines) became two `#[newtype(..)]` attributes.
- Validation lives in one function; `new` and `FromStr` both go through it, so there is no path that skips the check.
- Every generated trait is listed by name. Nothing is derived implicitly, so the newtype still hides exactly what it should (for example `Email` has no `DerefMut`).
