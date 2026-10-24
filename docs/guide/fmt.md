# `rustx-cli fmt`

`rustx-cli fmt` formats a `.rustx` file, or the `sugar! { .. }` bodies of a Rust file, in place with rustfmt, and keeps the sugar and the comments.

```sh
rustx-cli fmt <file.rustx | file.rs>
rustx-cli fmt --check <file.rustx | file.rs>
```

`--check` writes nothing and exits with 1 when the file is not formatted:

```text
a.rustx: not formatted; `rustx-cli fmt` rewrites it
```

rustfmt parses only Rust, so each sugar spelling is swapped for a placeholder that is valid Rust, rustfmt formats the result, and the sugar is put back. The layout is the one rustfmt gives ordinary Rust, and every sugar item is handled. The result is kept only when it expands to exactly what the file expanded to before; otherwise the file is left as it is, with an error. rustfmt comes from the toolchain (`rustup component add rustfmt`).

## Rust files

In a `.rs` file every `sugar! { .. }` and `rustx::sugar! { .. }` body is formatted, indented like its invocation, and the rest of the file is kept as written. For this file:

```rust
//! A comment outside stays as written.
fn   untouched( ) {}

rustx::sugar! {
    fn total(xs:&[u32])->u32{var sum=0;for x in xs{sum+=x;}sum}
}
```

`rustx-cli fmt lib.rs` leaves it as:

```rust
//! A comment outside stays as written.
fn   untouched( ) {}

rustx::sugar! {
    fn total(xs: &[u32]) -> u32 {
        var sum = 0;
        for x in xs {
            sum += x;
        }
        sum
    }
}
```

## Refusals

A file that does not expand is refused with the expansion's error, and a file rustfmt cannot parse with rustfmt's, both at their position, with exit code 1 and the file untouched. `gen` is a keyword of edition 2024, which rustfmt rejects as a name:

```text
refused.rustx:3:9: rustfmt cannot parse this, probably sugar `rustx-cli fmt` does not handle yet: expected identifier, found reserved keyword `gen`
```

## Limits

- `bound` and `derive` preset items are kept as written; the code around them is formatted.
- Lines that rustfmt cannot format stay verbatim, as in ordinary Rust.
- Macro inputs and `macro_rules!` bodies stay verbatim. Even `vec!` can be shadowed by a
  user macro that distinguishes a trailing comma, so formatting must not add one. The code
  around the invocation is still formatted.
- In Rust files, only actual `sugar!` calls in the syntax tree are formatted, including
  `::rustx::sugar!` and parenthesised or bracketed invocations. Tokens inside another macro
  are opaque: `inspect!(sugar! { struct S { a: u8 } })` is left as written because
  `inspect!` may match those tokens without ever invoking `sugar!`.
