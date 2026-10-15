# cargo-rustx

GitHub: https://github.com/pyrlyn/cargo-rustx

A syntax layer over Rust: sugar desugars into ordinary Rust through procedural macros built on `syn`.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T8 | in progress | P2 | 4 | 0% | Claude / opus-5.5 |
| T9 | in progress | P2 | 3 | 0% | Claude / opus-5.5 |
| T10.1 | in progress | P2 | 3 | 0% | Claude / opus-5.5 |
| T10.2 | todo | P2 | 4 | 0% | |

### T8. M3: `#[borrows]` structs and `&from(Self)`

Problem 19. `#[borrows] struct Lexer { src: &str }` adds one lifetime to the type, its fields and its `impl` headers; `&from(Self) T` / `&from(Type) T` resolve to that lifetime. Depends on T7.

Plan: own pass `passes/borrows.rs` (AST pass before `from_lifetimes::lower`); extend `from_lifetimes` so `from(Self)` / `from(Type)` resolve to the type's lifetime instead of "not a parameter". Unit tests, snapshot case, `pass/` compile test from the problem file, `fail/` cases (`#[borrows]` on a type with no references, two independent lifetimes, `from(Self)` outside a `#[borrows]` impl).

### T9. M3: `into T` parameters

Problem 15. `name: into String` becomes `name: impl Into<String>` plus `let name: String = name.into();` at the top of the body.

Plan: own pass `passes/into_params.rs` (token half on `tokens::params::map_fn_parameters` with a parameter marker, AST half rewrites the type and prepends the conversion). Unit tests, snapshot case, `pass/` compile test from the problem file, `fail/` cases (`into` without a type, `into` on `self`).

### T10.1. M3: parameter defaults in signatures

Problem 17 (parameters only; field defaults belong to M4). `port: u16 = 8080` becomes `port: Option<u16>` with `let port = port.unwrap_or(8080);` (or `unwrap_or_else` when the default is not a literal) at the top of the body.

Plan: own pass `passes/param_defaults.rs` (token half on `tokens::params::map_fn_parameters` with a parameter marker, AST half prepends the statement). Unit tests, snapshot case, `pass/` compile test calling with `Some(..)`/`None`, `fail/` cases (default before a parameter without one if disallowed, default on `self`).

### T10.2. M3: default and named arguments at call sites

Problem 17. Calls inside the same `sugar!` input to a fn with defaults: omitted trailing arguments become `None`, given ones `Some(..)`; named arguments `f(a, port: 9000)` are reordered to the declaration. Callers elsewhere see the real `Option<T>` signature. Depends on T10.1.
