# `throws` and `throw`

`-> T throws` returns `anyhow::Result<T>` (through `rustx`'s re-export), `-> T throws E` returns `Result<T, E>`; the tail is wrapped in `Ok(..)`, and `throw e` returns the error. Problem: [10, errors and `Result`](../../problems/10-errors-result.md).

```rust
rustx::sugar! {
    use std::num::ParseIntError;

    fn port(text: &str) -> u16 throws {
        if text.is_empty() {
            throw "empty port";
        }
        text.trim().parse()?
    }

    fn strict(text: &str) -> u16 throws ParseIntError {
        text.parse()?
    }
}

assert_eq!(port(" 80 ").unwrap(), 80);
assert_eq!(port("").unwrap_err().to_string(), "empty port");
assert!(strict("x").is_err());
```

Becomes:

```rust
use std::num::ParseIntError;
fn port(text: &str) -> ::rustx::anyhow::Result<u16> {
    if text.is_empty() {
        return ::core::result::Result::Err(::rustx::anyhow::anyhow!("empty port"));
    }
    ::core::result::Result::Ok(text.trim().parse()?)
}
fn strict(text: &str) -> ::core::result::Result<u16, ParseIntError> {
    ::core::result::Result::Ok(text.parse()?)
}
```

The tail is left alone when it already is `Ok(..)` or `Err(..)` or does not produce a value (`return`, `panic!`-like macros, a `loop` that no `break` leaves); a body without a value tail gets `Ok(())`. Under plain `throws`, `throw e` goes through `anyhow!(e)`, so a string literal is a format string; under `throws E` it is `Err(e.into())`. The crate needs `rustx` as a dependency, since the expansion names `::rustx::anyhow`.

## Limits

- The tail is wrapped as a whole; `if` and `match` arms are not entered.
- `throw` outside a `throws` fn is an error. Closures, `async` and `const` blocks and nested fns have their own signature: a `throw` inside them belongs to them.
- `throw (e)`, `throw &e` and `throw ::path` read as an identifier `throw`; start the value with a name, call or literal. A match arm with a leading `|` is read as a closure.
- `throws` without `->`, twice, or without a type is an error. Signatures inside macro bodies keep their marker, and a `break` inside a macro call is not seen.
