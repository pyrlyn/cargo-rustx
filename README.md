# BetterDiffuseRust

A thin syntax layer over Rust that removes the most disliked readability pain points. The syntax stays as close to Rust as possible and desugars into valid Rust through macros (a `#[sugar]` attribute / `sugar! {}` proc macro built on `syn`), with no custom compiler. Compiler errors keep pointing at the original spans.

## Problems

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
