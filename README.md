# cargo-rustx

cargo-rustx is a syntax layer over Rust. It stays as close to Rust as possible while simplifying the least readable parts: return-type lifetimes via `from`, `var` instead of `mut`, inline bounds in signatures, `~Trait` instead of `dyn Trait + Send + Sync`, `from` in enums instead of manual `impl From`, `.method<T>()` instead of turbofish, `#[delegate]` for trait delegation, `#[data]` for constructors and getters, `iter T` instead of `impl Iterator`, `throws` instead of `Box<dyn Error>`, `move[a, b]` for cloning captures, `shared` for `Arc<Mutex>`, automatic `into()` for safe conversions, and `async fn` in traits without `Pin`/`Box`.

The backend desugars into valid Rust through macros (`syn`), with no custom compiler. Single entry point: `#[sugar]` or `sugar!`.

Implementation plan: [ARCHITECTURE.md](ARCHITECTURE.md).

- [Lifetimes in return types](problems/01-return-lifetimes.md)
- [Ubiquitous `mut`](problems/02-ubiquitous-mut.md)
- [Long signatures with generics and `where`](problems/03-long-signatures.md)
- [`dyn Trait` in signatures](problems/04-dyn-trait.md)
- [Manual `impl` for simple cases](problems/05-manual-impl.md)
- [Turbofish](problems/06-turbofish.md)
- [Trait boilerplate: delegation](problems/07-trait-delegation.md)
- [Constructors and getters](problems/08-constructors-getters.md)
- [`impl Trait` and complex iterator returns](problems/09-impl-iterator.md)
- [Errors and `Result`](problems/10-errors-result.md)
- [Cloning for closures](problems/11-closure-clones.md)
- [`Arc<Mutex<T>>` and lock noise](problems/12-arc-mutex.md)
- [Explicit `as` and `into()`](problems/13-as-into.md)
- [`async` with `Pin` and `Box` in traits](problems/14-async-traits.md)
