# cargo-rustx

cargo-rustx is a syntax layer over Rust. It stays as close to Rust as possible and only simplifies the places people find hardest to read and write. Everything desugars into ordinary Rust through procedural macros built on `syn`; there is no custom compiler. There are two entry points: `sugar! { .. }` for the full vocabulary and attribute macros (`#[data]`, `#[display]`, `#[delegate]`, `#[newtype]`, `#[sugared]`) for code that is already valid Rust.

Status: early implementation. M0–M4 are in: `sugar!` handles `var` (02), `~Trait` (04), method generics without turbofish (06), `iter T` (09), `rc T`/`weak T` types (16), inline bounds (03), `bound` presets (21), `&from(x) T` lifetimes (01), `#[borrows]` types (19), `into T` parameters (15) and parameter defaults with default and named arguments at call sites (17), `derive` presets (18), field defaults with `S { .. }` (17), `#[data]` (08), `from` variants and `#[display]` (05), `#[delegate]` (07) and `#[newtype]` (20); the derive-like ones also work as standalone attributes, with `#[sugared]` for a whole item. Valid Rust passes through unchanged. Later milestones are in [roadmap.md](roadmap.md).

- [ARCHITECTURE.md](ARCHITECTURE.md): how it will be built.
- [research.md](research.md): why these items were chosen, with sources, and what upstream Rust already fixes.

## Usage

```rust
rustx::sugar! {
    fn total(xs: &[u32]) -> u32 { var sum = 0; for x in xs { sum += x; } sum }
}

#[rustx::data(builder)]
pub struct User { name: String, #[default(18)] age: u8 }

#[rustx::display]
#[derive(Debug)]
pub enum LoadError {
    #[display("io error: {0}")]
    Io(#[from] std::io::Error),
}
```

An attribute macro runs the one sugar item it names on the item it sits on, and sees nothing else. `#[sugared]` on an inline module runs every sugar item that is valid Rust (`#[data]`, `#[delegate(..)]` fields, `#[display("..")]`, `#[from]`, `#[newtype]`, `#[borrows]`) across the module, so `#[delegate]` can use the module's traits and `except(..)`, and `#[newtype(validate = f)]` its functions. It is not called `#[sugar]` because `sugar!` holds that name.

`#[dyn]` on a trait inside `sugar!` makes its `async fn`s callable through a generated `DynTrait` type (problem 14), via the `dynosaur` crate. It is behind an optional feature, so other users do not compile `dynosaur`: `rustx = { version = "..", features = ["dyn"] }`.

To see the plain Rust that sugar turns into, run `cargo run -p rustx-cli -- expand <file>`: a `.rustx` file is desugared as a whole, and in a Rust file every `sugar! { .. }` invocation is replaced by its expansion. The output is formatted with `prettyplease` (comments other than doc comments are dropped); errors are printed as `file:line:column: message` with a non-zero exit code.

To move existing code onto sugar, run `cargo run -p rustx-cli -- sugar <file.rs>`: it prints the file rewritten into sugar as the text of a `.rustx` file (save it as one, or paste it into `sugar! { .. }`). Only code that a sugar item expands back to exactly is rewritten (so far `let mut x` becomes `var x`, `.m::<T>()` becomes `.m<T>()`, `Rc::new(RefCell::new(e))` becomes `rc e`, and `x: Option<T>` unwrapped first thing by `let x: T = x.unwrap_or(v);` becomes `x: T = v`, together with every call to that fn in the file, which leaves out or names its defaulted arguments), and only those bytes change: comments and layout stay as written. Code that expansion would already change as written, such as `var x` inside a macro call, is reported as an error instead. Sugar expands to full std paths (`::std::rc::Rc::new`), and the check counts a short name such as `Rc` or `Option` as that path unless the file gives the name another meaning (its own item, a `use` from elsewhere, a generic parameter, a binding or a glob import).

To format a `.rustx` file, run `cargo run -p rustx-cli -- fmt <file.rustx>`: it rewrites the file in place with the layout `rustfmt` gives ordinary Rust, keeping the sugar and every comment (`--check` writes nothing and exits with 1 when the file is not formatted). It needs `rustfmt` on `PATH`, and refuses any change that would alter what the file expands to. Not formatted yet: `sugar! { .. }` bodies inside Rust files, and files that use `bound` or `derive` presets, inline bounds, parameter or call defaults, named arguments or capture lists, which it refuses with the position of the sugar.

### `.rustx` files via `build.rs`

A whole file can be written in sugar as a `.rustx` file and desugared by the build script. Add `rustx-build` to `[build-dependencies]` (and `rustx` to `[dependencies]` when the sugar needs its runtime, as `throws` does), then:

```rust
// build.rs
fn main() {
    if let Err(err) = rustx_build::expand_file("src/stats.rustx") {
        err.emit();
    }
}
```

```rust
// src/lib.rs
include!(concat!(env!("OUT_DIR"), "/src/stats.rs"));
```

The output sits under `OUT_DIR` at the source's path relative to the package root, with the extension `.rs`; `rustx_build::expand_dir("src")` desugars every `.rustx` file below a directory. The build script reruns when a desugared file, or a directory given to `expand_dir`, changes. `err.emit()` hands each error to Cargo as a `cargo::error` line, so the build fails with `error: <package>@<version>: src/stats.rustx:23:42: expected a trait after ...`; returning the error from `fn main() -> Result<(), rustx_build::Error>` also works but prints the same lines inside the build script's full output. `include!` rejects inner attributes, so `//!` docs and `#![..]` belong in the including file. rustc and clippy report problems in the generated code at the generated file, whose first line names its source. [examples/build-rs](examples/build-rs) is a working crate.

## Catalog

Each file shows the problem in short form, then a detailed example in valid Rust and the same code with sugar.

| # | Problem | Sugar |
| --- | --- | --- |
| 1 | [Lifetimes in return types](problems/01-return-lifetimes.md) | `-> &from(xs) Item` |
| 2 | [Ubiquitous `mut`](problems/02-ubiquitous-mut.md) | `var x = ..` |
| 3 | [Long signatures with generics and `where`](problems/03-long-signatures.md) | `x: T: Clone + Debug` |
| 4 | [`dyn Trait` in signatures](problems/04-dyn-trait.md) | `~Trait` |
| 5 | [Manual `impl` for simple cases](problems/05-manual-impl.md) | `Io(from io::Error)` |
| 6 | [Turbofish](problems/06-turbofish.md) | `.parse<u32>()`, `.collect<Vec>()` |
| 7 | [Trait boilerplate: delegation](problems/07-trait-delegation.md) | `#[delegate(Read)]` |
| 8 | [Constructors and getters](problems/08-constructors-getters.md) | `#[data]` |
| 9 | [`impl Trait` and complex iterator returns](problems/09-impl-iterator.md) | `-> iter &T` |
| 10 | [Errors and `Result`](problems/10-errors-result.md) | `-> T throws`, `throw e` |
| 11 | [Cloning for closures](problems/11-closure-clones.md) | `move[a.clone()] \|\| ..` |
| 12 | [`Arc<Mutex<T>>` and lock noise](problems/12-arc-mutex.md) | `shared x = ..` |
| 13 | [Explicit `as` and `into()`](problems/13-as-into.md) | implicit lossless `into()` |
| 14 | [`async` with `Pin` and `Box` in traits](problems/14-async-traits.md) | plain `async fn`, boxed for `dyn` |
| 15 | [`String` vs `&str` at API boundaries](problems/15-string-params.md) | `name: into String`, `"lit"` into `String` fields |
| 16 | [`Rc<RefCell<T>>` in trees and graphs](problems/16-rc-refcell.md) | `rc T`, `weak T` |
| 17 | [Default values](problems/17-default-values.md) | `port: u16 = 8080`, `S { a: 1, .. }` |
| 18 | [Walls of `#[derive(..)]`](problems/18-derive-walls.md) | `derive Id = Debug, Clone, ..;` |
| 19 | [Lifetime parameters spreading through structs](problems/19-struct-lifetimes.md) | `#[borrows] struct Lexer { src: &str }` |
| 20 | [Newtype boilerplate](problems/20-newtypes.md) | `#[newtype(deref, display, from_str)]` |
| 21 | [Repeated bound sets](problems/21-bound-aliases.md) | `bound Key = Clone + Eq + Hash;` |
