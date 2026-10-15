# `sugar!`

`rustx::sugar! { .. }` is the main entry point: its body may use every sugar item, and it expands to plain Rust where it stands.

```rust
rustx::sugar! {
    pub fn parse_all(text: &str) -> Vec<i32> throws {
        var all = Vec::new();
        for part in text.split(',') {
            all.push(part.trim().parse<i32>()?);
        }
        all
    }
}

assert_eq!(parse_all("1, 2").unwrap(), [1, 2]);
assert!(parse_all("x").is_err());
```

The body is a list of items, as in a file: fns, types, impls, modules, `use`. It may stand at module level or inside a fn body. Valid Rust passes through unchanged, so code can move into `sugar!` before it uses any sugar.

The sugar needs `sugar!` because rustc parses an item before an attribute macro sees it, and `var x`, `~Trait` or `-> T throws` do not parse as Rust. Sugar that is valid Rust (`#[data]`, `#[display]`, `#[delegate]`, `#[newtype]`, `#[borrows]`, `#[widen]`) also works without it, through the [attribute macros](attribute-macros.md) and [`#[sugared]`](sugared.md).

## Errors

A misuse is a compile error at the sugar token, with every error of the input reported together. Errors rustc finds in the expansion point at the sugar the code came from, because generated tokens keep its span. `rustx-cli expand` prints the expansion when such an error is hard to read; see [the CLI](cli.md).

## Limits

- A macro sees only its own input: parameter defaults, `derive` presets, `#[delegate]` with local traits and the other items that read declarations work within one `sugar!` only.
- Sugar inside macro arguments (`vec![..]`, `assert_eq!(..)`) is expanded too, so a user macro that accepts tokens like `var x` receives the desugared form.
- rust-analyzer expands the macro, so types and go-to-definition work, but completion and highlighting inside the body are limited, and rustfmt does not format macro bodies: [`rustx-cli fmt`](fmt.md) does. Whole files of sugar can be written as `.rustx` files instead: see [`build.rs`](build-rs.md).
