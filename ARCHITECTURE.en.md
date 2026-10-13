# Architecture (plan)

[Русская версия](ARCHITECTURE.md)

Status: plan only. Nothing is implemented yet.

## a) Approach: proc-macro crate vs standalone transpiler

| | Proc-macro crate (`sugar!` / `#[sugar]`) | Standalone transpiler (`.bdr` → `.rs`, build.rs or CLI) |
|---|---|---|
| Integration | `cargo add`, works in any crate, no build step | Needs build.rs / CLI, generated files, extra tooling |
| Input | Must tokenize as Rust tokens (`TokenStream`) | Arbitrary text, own lexer, any syntax |
| Error spans | Map back to source via `Span`; good on stable for most cases | Needs own source maps; rustc points at generated `.rs` unless remapped |
| IDE | rust-analyzer expands macros, but completion inside a custom-syntax body is weak | No IDE support for `.bdr` without a custom LSP |
| Type info | None | None (unless it calls rustc/rust-analyzer, which is out of scope) |
| Compile time | Macro runs on every build of the crate | Runs once per change, output is cached |

**Recommendation:** start with a proc-macro crate, function-like `sugar! { ... }` as the main path. Every sugar item in the vocabulary is expressible in Rust tokens, which removes the need for a custom lexer and gives span mapping for free. Keep the desugarer as a plain library (`bdr-core`) that works on `proc_macro2::TokenStream`, so a CLI transpiler (`bdr expand file.bdr`) can be added later on top of the same core without rewriting anything.

Crate layout:

- `bdr-core` — parser, AST, desugarer, diagnostics. Plain library, testable without proc-macro context.
- `bdr-macros` — thin `proc-macro = true` crate: `sugar!`, `#[sugar]`, `#[data]`, `#[delegate]`.
- `bdr` — facade crate re-exporting macros plus runtime helpers (`Shared<T>`, `throws` result alias).
- `bdr-cli` (later) — `bdr expand`, `bdr fmt`.

## b) Components

1. **Lexer.** None of our own: `proc_macro2::TokenStream` is the lexer. All sugar tokens (`var`, `~`, `throws`, `from`, `iter`, `shared`, `<` after a method name) are valid Rust tokens, just in invalid positions.
2. **Parser.** Two options:
   - Pure `syn` — only works when the input already parses as Rust. It does not (`var x = 1;` is not a statement).
   - **Custom parser built on `syn::parse::ParseStream`** (recommended): a fork of the relevant parts of the Rust grammar (items, fn signatures, types, statements, expressions) that accepts sugar forms and reuses `syn` for everything unchanged (`syn::Expr`, `syn::Type`, `syn::Path`, attributes). Sugar points are few and local, so the fork stays small: statement start (`var`, `shared`), type position (`~T`, `&from(x) T`, `iter T`, `shared T`), fn return (`T throws`), fn params (`x: T: Bound`), method calls (`.m<T>(`), closures (`move[..] ||`), expressions (`throw e`), enum variant fields (`from T`).
3. **AST.** `syn` types plus a small set of sugar node types (`SugarType::Boxed`, `SugarType::Borrow { from: Vec<Ident> }`, `SugarStmt::Var`, `SugarFn { throws: bool, inline_bounds }`, ...). Alternatively, parse sugar directly into `syn` nodes with marker attributes/placeholders and desugar in a second pass; this keeps the AST 100% `syn` and lets us use `syn::visit_mut`. Preferred: sugar AST only at the edges, lowered into `syn` immediately where the desugaring is local, and markers where it needs context (lifetimes, `throws` tail `Ok(..)`).
4. **Desugarer.** A set of independent passes, one per item, each a `VisitMut` over `syn::File`/`syn::ItemMod`:
   - `var` → `let mut`; `shared x = e` → `let x = Shared::new(e)`.
   - `~T` → `Box<dyn T + Send + Sync>`.
   - `&from(a) T` → introduce `'__a`, add to generics, rewrite param `a` to `&'__a`.
   - inline bounds → hoist into `Generics` / `where`.
   - `.m<T>(` → `.m::<T>(`, fill missing args with `_` for known std collections.
   - `T throws` → `bdr::Result<T>` (= `anyhow::Result<T>`), wrap tail in `Ok(..)`, `throw e` → `return Err(e.into())`.
   - `move[a.clone(), ..] || body` → `{ let a = a.clone(); move || body }`.
   - `iter T` → `impl Iterator<Item = T> + '_` (or with the `from` lifetime).
   - `async fn` in traits → native AFIT, or `async_trait`-style boxing when the trait is used as `dyn`/`~`.
   - `#[data]`, `#[delegate]`, `from` variants, `#[display]` → generated `impl` items (derive-like, can also be separate attribute macros).
   - implicit `into()` → only where the target type is syntactically known (see limitations).
   Pass order matters: inline bounds and `from` lifetimes before `iter T`; `throws` after `throw`; derives last.
5. **Diagnostics / pretty-printer.** Every generated token carries the span of the sugar token it came from. Errors detected by the desugarer are emitted as `compile_error!` with that span; errors detected by rustc point at the original code because spans are preserved. `bdr-cli expand` uses `prettyplease` for readable output.

## c) Implementation language

Rust, with `syn` (full + visit-mut + extra-traits), `quote`, `proc-macro2`, `proc-macro-error2` or manual `compile_error!`, `prettyplease` for the CLI. Why:

- Proc macros must be Rust anyway.
- `syn` already parses 95% of what we need; we only write the sugar delta.
- `proc_macro2` makes the core testable outside the compiler (unit and snapshot tests).
- One language for users and contributors; no runtime dependency on Python/JS tooling.

## d) Entry points: `#[sugar]` vs `sugar!`

Key constraint: **an attribute macro on an item receives the item only after rustc has parsed it as valid Rust.** `#[sugar] mod m { fn f() { var x = 1; } }` fails in the parser before the macro runs. The same applies to `~Trait` in a type, `-> T throws`, `x: T: Clone`, `&from(a) T`, `move[..] ||`, `.parse<u32>()` (which parses as comparisons and fails or, worse, changes meaning).

Therefore:

- **`sugar! { ... }` (function-like) is the main path.** The body is an arbitrary token tree; it only needs balanced delimiters. Flow: `TokenStream` → custom `ParseStream` parser → sugar AST → lowering into `syn::File` → `VisitMut` passes → `quote!` → output.
- **`#[sugar]` (attribute) is the secondary path** for code that is already valid Rust and uses only attribute-shaped sugar: `#[data]`, `#[delegate]`, `#[display]`, `from` on enum variants (spelled `#[from]`), and normalizations that act on valid syntax (e.g. `async fn` in traits → boxed variant, inline `Shared<T>`). Flow: `syn::parse::<Item>` → `VisitMut` → `quote!`.
- A whole-file form: `sugar! { include!("foo.bdr") }` is not possible (include! is expanded later); a file-level form needs the CLI/build.rs transpiler — a later stage.

## e) Error handling and span mapping

- Keep spans: every rewritten node is built with `quote_spanned!(orig.span()=> ...)`, so `var x` → `let mut x` points at `var`.
- Hygiene: use `Span::mixed_site()` for helper locals (`__bdr_tmp`) and introduced lifetimes so they cannot collide with user names; use `Span::call_site()`/user spans for user-visible identifiers.
- `Span::resolved_at` / `located_at` (stable on `proc_macro2`) combine "resolve like the macro, report like the user token".
- Desugarer errors (e.g. `from(y)` names no parameter, `throws` on `main` without `Result`) → `syn::Error::new(span, msg).to_compile_error()`; collect several with `syn::Error::combine` instead of stopping at the first.
- `Span::join` (to underline a whole range) is nightly-only on `proc_macro`; on stable fall back to the first token's span. Document this.
- rustc type errors inside generated code: we cannot rewrite them; best effort is good spans plus naming generated items predictably (`__bdr_*`) so messages stay readable.

## f) Testing

- **Snapshot tests (`insta`)** in `bdr-core`: `tests/cases/*.bdr` → expanded Rust, formatted with `prettyplease`, stored as `.snap`. One case per sugar item plus combined cases (the detailed examples from `problems/` are the first fixtures).
- **Compile tests (`trybuild`)**: `tests/pass/*.rs` must compile; `tests/fail/*.rs` must fail with a given `.stderr` (bad `from(..)`, `throws` misuse, `move[..]` with non-ident). Checks that error spans point at the right place.
- **Roundtrip tests**: as-is code from each `problems/*.md` must compile (already done manually in a scratch crate), and the desugared output must be equivalent to it (compare `prettyplease` output or run behavioural unit tests).
- **`cargo expand`** for manual debugging; `bdr-cli expand` later.
- Property test (later): any valid Rust input to `sugar!` passes through unchanged (sugar is a strict superset).

## g) Roadmap

1. **M0 skeleton:** workspace, `bdr-core`, `bdr-macros`, `sugar!` that passes valid Rust through unchanged; insta + trybuild wired; CI.
2. **M1 statements:** `var` → `let mut` (item 2). Establishes the custom parser for statements and span plumbing.
3. **M2 types (purely syntactic):** `~Trait` (4), `iter T` (9), turbofish `.m<T>()` (6).
4. **M3 signatures:** inline bounds (3), `&from(x) T` lifetimes (1), `from` combined with `iter`.
5. **M4 derives:** `#[data]` (8), `from` variants + `#[display]` (5), `#[delegate]` (7). Also usable as standalone attribute macros.
6. **M5 errors:** `T throws` + `throw` + tail `Ok(..)` (10).
7. **M6 closures and sharing:** `move[..]` (11), `shared` + `Shared<T>` runtime type (12).
8. **M7 async:** `async fn` in traits, boxed for dyn use (14).
9. **M8 conversions:** implicit `into()` in syntactically typed positions only (13); explicit opt-in.
10. **M9 tooling:** `bdr-cli expand`/`fmt`, file-level `.bdr` via build.rs, docs.

## h) Limitations

- **No type information.** Macros see tokens, not types. Implicit `into()` (13) can only fire where the target type is written in the source (`let x: u64 = ...`, struct literal of a struct declared in the same `sugar!`). Field `+=` needs the field type from the same macro input. Anything else stays explicit.
- **No type inference.** `collect<Vec>` → `Vec<_>` works for known names only; for user generic types the arity is unknown unless declared in the same input.
- **No cross-item info.** A macro sees only its own input. `#[delegate(Read)]` must know the trait's methods: works for a built-in list of std traits (`Read`, `Write`, `Iterator`, `Display`, ...) or traits declared in the same `sugar!`; otherwise the user lists methods explicitly.
- **`shared` field access** inserting `.lock().unwrap()` per access is not atomic across statements; needs a `with(|m| ..)` form and clear docs.
- **IDE / rust-analyzer:** rust-analyzer expands proc macros, so go-to-definition and types work on the output, but completion and highlighting inside `sugar!{}` are limited, and rustfmt does not format macro bodies. A `bdr fmt` tool is needed.
- **Nightly-only span APIs:** `Span::join`, `Span::source_text` (partially), `Diagnostic` with notes/help are unstable; stable output uses `compile_error!` with single-token spans.
- **Attribute path cannot carry syntax sugar** (see d); only the function-like path supports the full vocabulary.
- **Compile time:** `syn` with `full` features plus a parser adds build time to every user crate.
- **Semantics drift:** each sugar item must stay a pure, documented, local rewrite; anything that changes behaviour silently (e.g. lossy conversions) is out of scope.
