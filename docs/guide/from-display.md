# `from` variants and `#[display]`

`Io(from io::Error)` generates `From<io::Error>` for the enum; `#[display("..")]` on every variant generates `Display` and `Error`. Problem: [05, manual `impl` for simple cases](../../problems/05-manual-impl.md).

```rust
rustx::sugar! {
    use std::io;
    use std::num::ParseIntError;

    #[derive(Debug)]
    pub enum LoadError {
        #[display("io error: {0}")]
        Io(from io::Error),
        #[display("bad number: {0}")]
        Parse(from ParseIntError),
        #[display("missing key {key}")]
        Missing { key: String },
    }
}

fn parse(text: &str) -> Result<u16, LoadError> {
    Ok(text.trim().parse()?)
}

assert_eq!(parse("80").unwrap(), 80);
assert!(parse("x").unwrap_err().to_string().starts_with("bad number: "));
assert_eq!(LoadError::Missing { key: "port".into() }.to_string(), "missing key port");
```

Becomes:

```rust
use std::io;
use std::num::ParseIntError;
#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    Parse(ParseIntError),
    Missing { key: String },
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
            Self::Io(__rustx_arg0) => {
                ::core::write!(__rustx_f, "io error: {0}", __rustx_arg0)
            }
            Self::Parse(__rustx_arg0) => {
                ::core::write!(__rustx_f, "bad number: {0}", __rustx_arg0)
            }
            Self::Missing { key: __rustx_arg0, .. } => {
                ::core::write!(__rustx_f, "missing key {0}", __rustx_arg0)
            }
        }
    }
}
impl ::std::error::Error for LoadError {}
```

Placeholders are positional (`{0}`) for tuple variants and named (`{key}`) for struct variants, checked against the variant's fields. On valid Rust, outside `sugar!`, the same is [`#[rustx::display]`](attribute-macros.md) with `from` spelled `Io(#[from] io::Error)`.

## Limits

- No constructors are generated, although the problem file's implementation note mentions them.
- The generated `Error` has no `source()`, as in the problem file, and needs the enum to implement `Debug`. The impls copy the enum's generics without adding bounds: a generic enum states `T: Display + Debug` itself.
- `from` works on one-field variants only. Duplicate `from` types are found by their spelling, so `io::Error` and `std::io::Error` meet only in rustc's conflicting-impls error.
- `from (A, B)` and `from ::a::B` read as a type named `from`; write `from std::..` or alias the tuple.
- A variant without `#[display]`, a placeholder that names no field, and width or precision arguments (`{0:1$}`, `{:.*}`) are errors.
- `#[from]` in an enum that derives `Error` or `From` is left to that derive.
