# `.rustx` files through `build.rs`

A whole file can be written in sugar as a `.rustx` file and desugared by the build script with the `rustx-build` crate. [examples/build-rs](../../examples/build-rs) is a working crate; the blocks below are its files, so they are not compiled here (`ignore`): the workspace builds that crate, and its `tests/stats.rs` runs the desugared code.

```toml
[dependencies]
# Only when the sugar needs its runtime, as `throws` does.
rustx = ".."

[build-dependencies]
rustx-build = ".."
```

```rust,ignore
// build.rs: covered by examples/build-rs/build.rs
fn main() {
    if let Err(err) = rustx_build::expand_file("src/stats.rustx") {
        err.emit();
    }
}
```

```rust,ignore
// src/lib.rs: covered by examples/build-rs/src/lib.rs
include!(concat!(env!("OUT_DIR"), "/src/stats.rs"));
```

The output sits under `OUT_DIR` at the source's path relative to the package root, with the extension `.rs`. `rustx_build::expand_dir("src")` desugars every `.rustx` file below a directory. The build script reruns when a desugared file, or a directory given to `expand_dir`, changes.

## Errors

`err.emit()` hands each error to Cargo as a `cargo::error` line, so the build fails with the `.rustx` position:

```text
error: <package>@<version>: src/stats.rustx:23:42: expected a trait after ...
```

Returning the error from `fn main() -> Result<(), rustx_build::Error>` also works, but prints the same lines inside the build script's full output. rustc and clippy report problems in the generated code at the generated file, whose first line names its source.

## Limits

- `include!` rejects inner attributes, so `//!` docs and `#![..]` belong in the including file.
- Paths must be relative to the package, without `..`, valid UTF-8 and without line breaks.
- `#[data]` methods carry no docs, so a crate with `#![deny(missing_docs)]` fails on them.
