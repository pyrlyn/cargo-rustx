# Field defaults and `S { .. }`

`port: u16 = 8080` on a struct field generates `impl Default`, and a struct literal ending in a bare `..` takes the rest from it. The syntax is RFC 3681's, so the pass can switch to the native form once it is stable. Problem: [17, default values](../../problems/17-default-values.md).

```rust
rustx::sugar! {
    #[derive(Debug)]
    struct Config {
        host: String = "localhost",
        port: u16 = 8080,
        verbose: bool,
    }

    fn demo() -> Config {
        Config { verbose: true, .. }
    }
}

let config = demo();
assert_eq!((config.host.as_str(), config.port, config.verbose), ("localhost", 8080, true));
```

Becomes:

```rust
#[derive(Debug)]
struct Config {
    host: String,
    port: u16,
    verbose: bool,
}
impl ::core::default::Default for Config {
    fn default() -> Self {
        Self {
            host: ::core::convert::Into::into("localhost"),
            port: 8080,
            verbose: ::core::default::Default::default(),
        }
    }
}
fn demo() -> Config {
    Config {
        verbose: true,
        ..::core::default::Default::default()
    }
}
```

A field without a default takes `Default::default()`. Generics, `where` and `cfg` are copied to the impl.

## Limits

- `derive(Default)` next to field defaults is an error, and so are defaults on enum variant or union fields.
- A bare `..` becomes `..Default::default()` in every struct expression, including structs declared outside the input; inside macro arguments only for std expression macros (`vec!`, `assert_eq!`, `format!`, ...), never in `matches!` or user macros. Struct patterns keep their `..`.
- Bounds are copied, not inferred: a generic field without a default needs `T: Default` on the struct.
- Field defaults do not parse on stable Rust, so [attribute macros](attribute-macros.md) cannot reach them; use `sugar!`. Inside `sugar!`, [`#[data]`](data.md) reads them for `new` too; on valid Rust it reads `#[default(expr)]`.
