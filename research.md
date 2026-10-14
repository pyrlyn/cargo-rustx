# Research: where Rust is hard to read

Why the catalog in `problems/` contains what it contains, and which items upstream Rust is already fixing. All sources were checked on 2026-10-15. At that date the latest stable release was Rust 1.99.0 (https://blog.rust-lang.org/releases/). The examples in `problems/` were compiled with Rust 1.97.1, edition 2024.

Facts backed only by secondary sources are marked **unverified**.

## What users report

### State of Rust surveys

- **2023**, 9,374 respondents (https://blog.rust-lang.org/2024/02/19/2023-Rust-Annual-Survey-2023-results/):
  - The top worry is "Rust becoming too complex" at 43%, up 5 points from 2022.
  - The hardest areas named are async Rust, the traits and generics system, and the borrow checker.
  - Wanted features include trait aliases, associated type defaults and async closures.
- **2024** (https://blog.rust-lang.org/2025/02/13/2024-State-Of-Rust-Survey-results/):
  - Complexity stays among the top worries.
  - About 31% of non-users give the perception of difficulty as their main reason.
  - The most desired unstable features were async closures and let chains.
- **2025**, 7,156 responses (https://blog.rust-lang.org/2026/03/02/2025-State-Of-Rust-Survey-results):
  - Concerns about growing complexity persist.
  - Let chains and async closures, both stabilized in 2025, are reported as widely used.
  - The most wanted features are generic const expressions and improved trait methods.

### Rust project goals and documentation

- The 2026 roadmaps (https://goals.rust-lang.org/2026/roadmaps.html) include "Just add async" (async traits, closures, drop), "Beyond the &" (smart-pointer ergonomics) and "The Borrow Checker Within".
- The 2026 goal "ergonomic ref-counting" (https://goals.rust-lang.org/2026/ergonomic-rc.html) targets clone-before-`move` boilerplate.
- The Rust Book says new Rustaceans "commonly get stuck on strings" (https://doc.rust-lang.org/book/ch08-02-strings.html).

### How items map to evidence

| # | Item | Evidence |
| --- | --- | --- |
| 1, 19 | Lifetimes in signatures, structs and impls | 2023 survey: borrow checker among the hardest areas; 2026 roadmap "The Borrow Checker Within" |
| 3, 21 | Long bounds; repeated bound sets | 2023 survey: traits and generics among the hardest areas, trait aliases wanted |
| 9 | `impl Iterator` returns | Edition 2024 capture rules changed precisely this (see status table) |
| 11, 12, 16 | Clones before `move`, `Arc<Mutex>`, `Rc<RefCell>` | 2026 goal "ergonomic ref-counting"; roadmap "Beyond the &" |
| 14 | async in traits | 2023 survey: async among the hardest areas; 2026 goal for `dyn` async traits |
| 15 | `String` vs `&str` | Rust Book, chapter 8.2 |
| 17 | Default values | RFC 3681 motivation (https://github.com/rust-lang/rfcs/pull/3681) |
| 2, 4–8, 10, 13, 18, 20 | Other boilerplate | No survey figure. These are recurring boilerplate patterns, and the existence of `thiserror`, `derive_more` and `anyhow` shows the demand. Treat as observation. |

## Upstream status

What upstream Rust already offers or is building for each item. "Stable" means usable on stable Rust today. Where upstream is stable, the sugar is only a shorter spelling. Where it is nightly-only, the sugar stays justified until it lands.

| Feature | Status on 2026-10-15 | Affects | Source |
| --- | --- | --- | --- |
| `async fn` in traits | Stable since 1.75 (2023-12-28) | 14 | https://blog.rust-lang.org/2023/12/28/Rust-1.75.0/ |
| Native `dyn` async traits | Not available. Accepted 2026–2027 goal. `dynosaur` 0.3.1 is the maintained workaround. | 14 | https://goals.rust-lang.org/2026/afidt-box.html, https://crates.io/api/v1/crates/dynosaur |
| Return type notation (`T::m(..): Send`) | Nightly only | 14 | https://github.com/rust-lang/rust/issues/109417 |
| Edition 2024 RPIT lifetime capture | Stable (1.85). `+ '_` is no longer required on edition 2024. | 9 | https://doc.rust-lang.org/edition-guide/rust-2024/rpit-lifetime-capture.html |
| `use<..>` precise capturing | Stable (1.82) | 9 | https://blog.rust-lang.org/2024/10/17/Rust-1.82.0/ |
| Async closures, `AsyncFn*` | Stable (1.85) | 11, 14 | https://blog.rust-lang.org/2025/02/20/Rust-1.85.0/ |
| Let chains | Stable (1.88), edition 2024 only | none (not in the catalog) | https://blog.rust-lang.org/2025/06/26/Rust-1.88.0/ |
| `.use` / `UseCloned` (RFC 3680) | RFC open, nightly `ergonomic_clones`, no longer the leading proposal | 11, 12 | https://github.com/rust-lang/rfcs/pull/3680, https://github.com/rust-lang/rust/issues/132290 |
| `move(expr)` capture expressions (RFC 3968) | RFC open, nightly `move_expr` | 11 | https://github.com/rust-lang/rfcs/pull/3968 |
| Default field values (RFC 3681) | RFC merged, nightly `default_field_values` | 17 | https://github.com/rust-lang/rust/issues/132162 |
| Named or default function arguments | No RFC. Only a pre-RFC on internals (**unverified**). | 17 | https://internals.rust-lang.org/t/pre-rfc-function-parameter-defaults/24011 |
| Trait aliases | Nightly `trait_alias` | 21 | https://github.com/rust-lang/rust/issues/41517 |
| TAIT / ATPIT | Nightly, blocked on the next-gen trait solver | 9 | https://github.com/rust-lang/rust/issues/63063 |
| `try` blocks | Nightly `try_blocks` | 10 | https://github.com/rust-lang/rust/issues/154391 |
| `Iterator::try_collect` | Nightly `iterator_try_collect` | 6, 10 | https://github.com/rust-lang/rust/issues/94047 |
| Deref patterns (matching through `Box`/`String`) | Nightly `deref_patterns`. `box` patterns were removed in their favour. | none | https://github.com/rust-lang/rust/issues/87121, https://github.com/rust-lang/rust/issues/29641 |
| Postfix `.match` | RFC open, nightly `postfix_match` | none | https://github.com/rust-lang/rust/issues/121618 |
| Variadic generics | No active RFC (RFC 2775 closed in 2020) | none | https://github.com/rust-lang/rfcs/pull/2775 |
| `Rc<RefCell<T>>` ergonomics in std | No RFC, tracking issue or goal found | 16 | none found |

## Hard to read, but out of reach for a macro

These complaints are real but need type information, borrow-checker changes or compiler work. They are recorded here so they are not proposed again as sugar.

- **Reference soup in closures**: `.filter(|&&x| x > 0)`, `|&(a, b)|`, `**x`. Removing the `&` and `*` needs the element type, which a macro does not see. `.copied()` and `.cloned()` are the existing fix.
- **Borrow checker error messages** and lifetimes that cannot be expressed (self-referential structs). These are compiler concerns.
- **Orphan rule and coherence errors.** These are a type-system concern.
- **`macro_rules!` syntax** (`$x:expr`, repetitions). It is replaceable by proc macros, not by sugar.
- **Compile times.** The top problem in the 2025 survey. A macro layer makes this worse, not better (see [ARCHITECTURE.md](ARCHITECTURE.md), Limitations).
