# BetterDiffuseRust

## Русский

BetterDiffuseRust — язык-надстройка над Rust. Синтаксис максимально похож на Rust, но упрощает самые нечитаемые места: лайфтаймы в возвращаемых типах через `from`, `var` вместо `mut`, inline-ограничения в сигнатурах, `~Trait` вместо `dyn Trait + Send + Sync`, `from` в enum вместо ручного `impl From`, `.method<T>()` вместо turbofish, `#[delegate]` для делегирования трейтов, `#[data]` для конструкторов и геттеров, `iter T` вместо `impl Iterator`, `throws` вместо `Box<dyn Error>`, `move[a, b]` для захвата с клонированием, `shared` для `Arc<Mutex>`, автоматический `into()` при безопасных приведениях, `async fn` в трейтах без `Pin`/`Box`.

Бэкенд — десугаринг в валидный Rust через макросы (`syn`), без своего компилятора. Одна точка входа: `#[sugar]` или `sugar!`.

План реализации: [ARCHITECTURE.md](ARCHITECTURE.md).

- [Лайфтаймы в возвращаемом типе](problems/01-return-lifetimes.md)
- [Вездесущий `mut`](problems/02-ubiquitous-mut.md)
- [Длинные сигнатуры с generics и `where`](problems/03-long-signatures.md)
- [`dyn Trait` в сигнатурах](problems/04-dyn-trait.md)
- [Ручной `impl` для простых случаев](problems/05-manual-impl.md)
- [Turbofish](problems/06-turbofish.md)
- [Boilerplate вокруг трейтов: делегирование](problems/07-trait-delegation.md)
- [Конструкторы и геттеры](problems/08-constructors-getters.md)
- [`impl Trait` и сложные возвращаемые итераторы](problems/09-impl-iterator.md)
- [Ошибки и `Result`](problems/10-errors-result.md)
- [Клонирование в замыканиях](problems/11-closure-clones.md)
- [`Arc<Mutex<T>>` и lock-шум](problems/12-arc-mutex.md)
- [Явные `as` и `into()`](problems/13-as-into.md)
- [`async` с `Pin` и `Box` в трейтах](problems/14-async-traits.md)

## English

BetterDiffuseRust is a syntax layer over Rust. It stays as close to Rust as possible while simplifying the least readable parts: return-type lifetimes via `from`, `var` instead of `mut`, inline bounds in signatures, `~Trait` instead of `dyn Trait + Send + Sync`, `from` in enums instead of manual `impl From`, `.method<T>()` instead of turbofish, `#[delegate]` for trait delegation, `#[data]` for constructors and getters, `iter T` instead of `impl Iterator`, `throws` instead of `Box<dyn Error>`, `move[a, b]` for cloning captures, `shared` for `Arc<Mutex>`, automatic `into()` for safe conversions, and `async fn` in traits without `Pin`/`Box`.

The backend desugars into valid Rust through macros (`syn`), with no custom compiler. Single entry point: `#[sugar]` or `sugar!`.

Implementation plan: [ARCHITECTURE.en.md](ARCHITECTURE.en.md).

- [Lifetimes in return types](problems/01-return-lifetimes.en.md)
- [Ubiquitous `mut`](problems/02-ubiquitous-mut.en.md)
- [Long signatures with generics and `where`](problems/03-long-signatures.en.md)
- [`dyn Trait` in signatures](problems/04-dyn-trait.en.md)
- [Manual `impl` for simple cases](problems/05-manual-impl.en.md)
- [Turbofish](problems/06-turbofish.en.md)
- [Trait boilerplate: delegation](problems/07-trait-delegation.en.md)
- [Constructors and getters](problems/08-constructors-getters.en.md)
- [`impl Trait` and complex iterator returns](problems/09-impl-iterator.en.md)
- [Errors and `Result`](problems/10-errors-result.en.md)
- [Cloning for closures](problems/11-closure-clones.en.md)
- [`Arc<Mutex<T>>` and lock noise](problems/12-arc-mutex.en.md)
- [Explicit `as` and `into()`](problems/13-as-into.en.md)
- [`async` with `Pin` and `Box` in traits](problems/14-async-traits.en.md)
