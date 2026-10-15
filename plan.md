# cargo-rustx

GitHub: https://github.com/pyrlyn/cargo-rustx

A syntax layer over Rust: sugar desugars into ordinary Rust through procedural macros built on `syn`.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T8 | todo | P2 | 4 | 0% | |
| T9 | todo | P2 | 3 | 0% | |
| T10 | todo | P2 | 5 | 0% | |

### T8. M3: `#[borrows]` structs and `&from(Self)`

Problem 19. `#[borrows] struct Lexer { src: &str }` adds one lifetime to the type, its fields and its `impl` headers; `&from(Self) T` / `&from(Type) T` resolve to that lifetime. Depends on T7.

### T9. M3: `into T` parameters

Problem 15. `name: into String` becomes `name: impl Into<String>` plus `let name: String = name.into();` at the top of the body.

### T10. M3: parameter defaults and named arguments

Problem 17 (parameters only; field defaults belong to M4). `port: u16 = 8080` becomes `Option<u16>` with `unwrap_or`, calls inside the same `sugar!` input are filled in. Split when claimed.
