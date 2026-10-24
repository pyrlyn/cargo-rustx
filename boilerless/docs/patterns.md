# Rust boilerplate patterns: research notes

Notes on which boilerplate Rust developers keep writing by hand, why the existing crates do not
fully fix it, and how `boilerless` covers it.

Legend: **[P]** popular (in most codebases), **[R]** rare-but-frequent (less visible, but keeps
coming back across projects).

## Top 10 patterns

| # | Pattern | Kind | Written by hand | Existing crates |
|---|---------|------|-----------------|-----------------|
| 1 | Error enum | [P] | `impl Display` match, `impl Error`, one `impl From` per source | `thiserror`, `snafu`, `derive_more` |
| 2 | Builder | [P] | Builder struct of `Option`s, setter per field, `build()` with missing-field errors | `derive_builder`, `typed-builder`, `bon` |
| 3 | Enum <-> string | [P] | `as_str` match, `Display`, `FromStr` with an error arm | `strum`, `parse-display` |
| 4 | `From`/`TryFrom` for newtypes | [P] | Two or more `impl From` per wrapper | `derive_more`, `nutype` |
| 5 | Constructor `new` | [P] | `fn new(a, b, ..) -> Self { Self { a, b, .. } }` with defaults | `derive-new` |
| 6 | Getters / setters | [R] | One method per field, sometimes `&mut` chainable setters | `getset`, `derive-getters` |
| 7 | Newtype delegation | [R] | `Deref`, `DerefMut`, `AsRef`, `into_inner` | `derive_more`, `shrinkwraprs`, `delegate` |
| 8 | Method -> trait binding | [R] | Trait impl whose every method forwards to an inherent one | `delegate`, `ambassador` |
| 9 | Config from env | [R] | Key lookup, parse, default or error, per field | `envy`, `config`, `figment`, `clap` (env) |
| 10 | Variant registry | [R] | Hand-kept `ALL` array, count, index match | `strum` (`EnumIter`, `EnumCount`), `enum-iterator`, `inventory` |

## Why the existing crates are underused

- **Fragmentation:** a typical service needs 5-8 of these patterns. Each comes from a different
  crate with its own attribute namespace (`#[error]`, `#[builder]`, `#[strum]`, `#[getset]`, ...),
  docs and MSRV policy. Teams pick two or three and write the rest by hand.
- **Dependency and compile-time cost:** every proc-macro crate adds its own `syn` build. Several
  macro crates on one type also slow down rust-analyzer.
- **Over-generality:** crates like `derive_builder` and `figment` are very flexible, which makes
  their attributes hard to remember for the common 80% case.
- **Rare-but-frequent patterns go unnoticed:** delegation, registries and env config are each
  "only 20 lines", so nobody adds a dependency. They are still duplicated across many files.

## All-in-one compatibility

- **Single macro:** one `#[derive(Boil)]` with `#[boil(a, b, c)]` composes patterns on one type
  (builder + getters + setters + new on one struct is tested). Attribute macros cover what derives
  cannot reach (`#[bind]` on impl blocks). Generated names are kept apart so patterns do not clash
  (`builder()`, `x()`, `set_x()`, `new()`).
- **build.rs:** not needed. Everything is a regular proc macro with no codegen step, so it works
  with `cargo check`, `cargo doc` and tests out of the box. A `build.rs` route (as in
  `rustx-build`) would only matter for whole-file rewriting, which is out of scope.
- **IDE:** rust-analyzer expands derive and attribute macros when proc-macro support is on.
  `tools/ra-macros` switches that setting per project, for when expansion is slow or noisy.

## Architecture sketch

```
#[derive(Boil)] / #[bind(..)]        (macros/src/lib.rs: entry points)
        |
        v
parse.rs   #[boil(..)] -> Pattern / FieldConfig / VariantConfig (syn::meta)
        |
        v
rules.rs   RULES registry: name, description, syntax, accepted shape, generics, output
           check(rule, item): shape and generics validation -> compile_error on mismatch
        |
        v
codegen/<pattern>.rs   one generator per pattern -> TokenStream
        |
        v
facade crate `boilerless` re-exports the macros; tests: unit, codegen, runtime, trybuild ui + e2e
```

Errors are `syn::Error` spanned at the offending attribute or item and turned into
`compile_error!`, so users see e.g. ``unknown boil pattern `bulder`; expected one of: ...``.

## v1 coverage vs implemented

| Pattern | v1 target | Status |
|---------|-----------|--------|
| error | Display from `msg`, `Error`, `From` via `from` | done (no `source()`, tuple fields are `{_0}`) |
| builder | Option-based builder, defaults, skip, generics | done |
| enum_str | `as_str`, Display, FromStr, rename | done (unit enums only) |
| convert | `From` both ways, `TryFrom` | partial: `From` only, no generics |
| new | positional constructor with defaults | done |
| getters / setters | borrowing getters, chainable setters | done |
| newtype | Deref/DerefMut/AsRef/into_inner, Display delegation | partial: no Display or operator delegation |
| bind | trait impl delegating to inherent methods | done (signatures must match the trait) |
| config | env/lookup loading, prefix, defaults | partial: flat fields only, no nested or `Option` |
| registry | `ALL`, `COUNT`, `index`, `variant_name` | done (unit enums only) |
