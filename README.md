# cargo-rustx

cargo-rustx is a syntax layer over Rust. It stays as close to Rust as possible and only simplifies the places people find hardest to read and write. Everything desugars into ordinary Rust through procedural macros built on `syn`; there is no custom compiler. There are two entry points: `sugar! { .. }` for the full vocabulary and `#[sugar]` for code that is already valid Rust.

Status: early implementation. The workspace skeleton is in place; `sugar!` passes valid Rust through unchanged and sugar items land milestone by milestone (see [plan.md](plan.md)).

- [ARCHITECTURE.md](ARCHITECTURE.md): how it will be built.
- [research.md](research.md): why these items were chosen, with sources, and what upstream Rust already fixes.

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
