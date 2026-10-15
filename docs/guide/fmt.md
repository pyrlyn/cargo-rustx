# `rustx-cli fmt`

`rustx-cli fmt` formats a `.rustx` file in place with rustfmt and keeps its sugar and comments.

```sh
rustx-cli fmt <file.rustx>
rustx-cli fmt --check <file.rustx>
```

`--check` writes nothing and exits with 1 when the file is not formatted:

```text
a.rustx: not formatted; `rustx-cli fmt` rewrites it
```

rustfmt parses only Rust, so each sugar spelling is swapped for a placeholder that is valid Rust, rustfmt formats the result, and the sugar is put back. The layout is the one rustfmt gives ordinary Rust. The result is kept only when it expands to exactly what the file expanded to before; otherwise the file is left as it is, with an error. rustfmt comes from the toolchain (`rustup component add rustfmt`).

## Refusals

Sugar that `fmt` cannot map yet reaches rustfmt as it is; rustfmt cannot parse it, and `fmt` reports where, with exit code 1 and the file untouched:

```text
c.rustx:4:7: rustfmt cannot parse this, probably sugar `rustx-cli fmt` does not handle yet: expected one of `!` or `::`, found `Key`
```

## Limits

- Handled: `var` and `shared` bindings, `throw e`, `rc e`, the type prefixes (`iter`, `rc`, `weak`, `shared`, `into T`, `from T`, `&from(a) T`), `~T`, `#[dyn]`, `.m<T>(` and `throws`.
- Not yet: `bound` and `derive` presets, inline bounds, parameter defaults with call-site defaults and named arguments, and `move[..]` capture lists.
- Only `.rustx` files: `sugar!` bodies in `.rs` files are not formatted yet, and a Rust file is refused.
