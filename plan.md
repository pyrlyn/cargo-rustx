# cargo-rustx

GitHub: https://github.com/pyrlyn/cargo-rustx

A syntax layer over Rust: sugar desugars into ordinary Rust through procedural macros built on `syn`.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T5 | in progress | P1 | 3 | 0% | Claude / opus-5.5 |
| T6 | in progress | P1 | 3 | 0% | Claude / opus-5.5 |
| T7 | in progress | P1 | 4 | 0% | Claude / opus-5.5 |
| T8 | todo | P2 | 4 | 0% | |
| T9 | todo | P2 | 3 | 0% | |
| T10 | todo | P2 | 5 | 0% | |

### T5. M3: inline bounds in parameters

Problem 03. `x: T: Clone + Debug` in fn parameters hoists the bound into the fn's generics / `where` clause; the generic parameter is declared once, existing generics and `where` clauses are kept.

Plan: own pass module `passes/inline_bounds.rs` (token pass that turns the sugar into valid Rust, e.g. a parameter attribute marker, plus an AST pass that moves it into `where`). Unit tests, snapshot case, `pass/` compile test from the problem file, `fail/` cases for misuse (bound on a non-generic type, conflicting bounds).

### T6. M3: `bound` presets

Problem 21. `bound Key = Clone + Eq + Hash;` at item level becomes a trait with those supertraits plus a blanket impl, usable everywhere a bound is.

Plan: own pass module `passes/bound_presets.rs` (token pass at item position). Unit tests, snapshot case, `pass/` compile test from the problem file, `fail/` cases (missing `=`, empty bound list, generics on the preset if not supported).

### T7. M3: `&from(x) T` return lifetimes

Problems 01 and 09 (`from` combined with `iter T`). `-> &from(xs) Item` introduces a lifetime, adds it to the fn generics and to the reference parameter `xs`; `from(a, b)` ties several parameters; `iter T` with `from(..)` uses that lifetime instead of `'_`.

Plan: own pass module `passes/from_lifetimes.rs`. Must run before `prefix_types` expands `iter T` or adjust its `'_` afterwards; the order change in `passes/mod.rs` is done by the coordinator. Unit tests, snapshot case, `pass/` compile test from the problem files, `fail/` cases (`from(y)` names no parameter, `from` on a non-reference parameter).

### T8. M3: `#[borrows]` structs and `&from(Self)`

Problem 19. `#[borrows] struct Lexer { src: &str }` adds one lifetime to the type, its fields and its `impl` headers; `&from(Self) T` / `&from(Type) T` resolve to that lifetime. Depends on T7.

### T9. M3: `into T` parameters

Problem 15. `name: into String` becomes `name: impl Into<String>` plus `let name: String = name.into();` at the top of the body.

### T10. M3: parameter defaults and named arguments

Problem 17 (parameters only; field defaults belong to M4). `port: u16 = 8080` becomes `Option<u16>` with `unwrap_or`, calls inside the same `sugar!` input are filled in. Split when claimed.
