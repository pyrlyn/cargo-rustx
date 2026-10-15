# cargo-rustx user guide

cargo-rustx is a syntax layer over Rust: sugar desugars into ordinary Rust through procedural macros. Each page below shows one sugar item, the plain Rust it becomes (the real output of `rustx-cli expand`) and its limits. The problem files under [problems/](../../problems) explain why each item exists. Every Rust example here is compiled and run as a doc test of the `rustx` crate.

## Getting started

```toml
[dependencies]
rustx = ".."
```

## Entry points

- [`sugar!`](sugar-macro.md): the full vocabulary, in any crate.
- [Attribute macros](attribute-macros.md): `#[data]`, `#[display]`, `#[delegate]`, `#[newtype]` on valid Rust.
- [`#[sugared]`](sugared.md): every attribute-shaped item across a module.
- [`.rustx` files through `build.rs`](build-rs.md): whole files of sugar, with `rustx-build`.
- [`rustx-cli expand` and `rustx-cli sugar`](cli.md): show the expansion, or move plain Rust onto sugar.
- [`rustx-cli fmt`](fmt.md): format `.rustx` files.
- [The `dyn` feature](dyn-feature.md): needed by `#[dyn]` traits.

## Sugar items

| # | Item | Sugar |
| --- | --- | --- |
| 1 | [Return lifetimes](from-lifetimes.md) | `-> &from(xs) T` |
| 2 | [`var` bindings](var.md) | `var x = 0;` |
| 3 | [Inline bounds](inline-bounds.md) | `x: T: Clone + Debug` |
| 4 | [Boxed trait objects](boxed-traits.md) | `~Trait` |
| 5 | [`from` variants and `#[display]`](from-display.md) | `Io(from io::Error)`, `#[display("..")]` |
| 6 | [Method generics without turbofish](turbofish.md) | `.parse<u32>()`, `.collect<Vec>()` |
| 7 | [`#[delegate]` fields](delegate.md) | `#[delegate(Write)] inner: W` |
| 8 | [`#[data]`](data.md) | `#[data(builder)] struct User { .. }` |
| 9 | [`iter T` returns](iter.md) | `-> iter &T` |
| 10 | [`throws` and `throw`](throws.md) | `-> T throws`, `throw e` |
| 11 | [Capture lists](capture-lists.md) | `move[a.clone()] \|\| ..` |
| 12 | [`shared` bindings](shared.md) | `shared s = e;` |
| 13 | [`#[widen]` conversions](widen.md) | `let x: u64 = small;` |
| 14 | [`#[dyn]` traits](dyn-traits.md) | `#[dyn] trait Store { async fn .. }` |
| 15 | [`into T` parameters](into-params.md) | `name: into String` |
| 15 | [String literals into `String`](string-literals.md) | `Contact { name: "Ann" }` |
| 16 | [`rc T`, `weak T`, `rc e`](rc.md) | `children: Vec<rc Node>` |
| 17 | [Parameter defaults and named arguments](param-defaults.md) | `port: u16 = 5432`, `f(x, port: 1)` |
| 17 | [Field defaults](field-defaults.md) | `port: u16 = 8080`, `S { a: 1, .. }` |
| 18 | [`derive` presets](derive-presets.md) | `derive Id = Debug, Clone;` |
| 19 | [`#[borrows]` types](borrows.md) | `#[borrows] struct Lexer { src: &str }` |
| 20 | [`#[newtype]`](newtype.md) | `#[newtype(deref, display)]` |
| 21 | [`bound` presets](bound-presets.md) | `bound Key = Clone + Eq + Hash;` |

The number is the problem file's. General limits (no type information, one macro sees only its own input) are in [ARCHITECTURE.md](../../ARCHITECTURE.md), section h.
