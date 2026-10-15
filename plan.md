# cargo-rustx

GitHub: https://github.com/pyrlyn/cargo-rustx

A syntax layer over Rust: sugar desugars into ordinary Rust through procedural macros built on `syn`.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T19 | todo | P2 | 5 | 0% | |
| T29 | in progress | P3 | 3 | 0% | Claude / opus-5.5 |
| T31 | in progress | P3 | 3 | 0% | Claude / opus-5.5 |
| T33 | todo | P3 | 2 | 0% | |

### T19. Convert plain Rust into rustx sugar

The reverse of `expand`: take existing Rust written without sugar and rewrite it into the rustx form, so old code can adopt the syntax without a manual rewrite. One reverse rule per sugar item that has a pass; a rule fires only when the code is exactly what that pass would generate (e.g. `x: Option<T>` + `unwrap_or(..)` back to `x: T = ..`, `let mut` back to `var`). Anything it cannot prove equivalent stays as written.

Done when: a `rustx-cli sugar <file>` command (or the M9 CLI, if it lands first) prints the converted source; for every reverse rule a test shows `expand(sugar(x))` is token-equal to `x` after normalisation, and valid Rust with no matching shape comes back unchanged; snapshot cases cover each rule. Too big for one task: split into a shared driver plus one subtask per sugar item when claimed.

Split when claimed: T19.1 is the shared driver with the first rule; the other reverse rules become T19.2 onward, one per sugar item, once the driver has landed.

### T29. M8: string literals into `String`

Problem 15. A string literal gets `.to_string()` where the target type is written as `String` in the same input (struct fields, `Option<String>`, `Vec<String>`, `let x: String`), on the machinery of T28 and the shared `is_string`. Depends on T28. Done when the problem's `Contact` example compiles, with unit tests, a snapshot case and `pass/` tests, and positions whose type is not written stay unchanged.

Plan: always on, without an opt-in: a string literal in a position whose type is written `String` does not compile as plain Rust, so the rewrite cannot change the meaning of valid Rust. AST pass `passes/string_literals.rs::lower` (pre-registered before `widen::lower`) implements `typed_positions::Convert`: a literal, `Some(lit)` or `vec![lit, ..]` meeting `String`, `Option<String>` or `Vec<String>` (the shared `is_string`) gets `::std::string::String::from(lit)`; it adds the struct-literal field position to `typed_positions` (shorthand fields get their `:` back). Verify: unit tests, `tests/cases/string_literals.rustx`, `pass/string_literals.rs` with the problem's `Contact` example, `just check`.

### T31. M9: `rustx-cli fmt`

Formatting for sugar sources, so `.rustx` files and `sugar!` bodies get a consistent layout without rustfmt (which rejects the sugar). Scope (round-trip through the parser or a token-level formatter, which constructs) is settled with the creator when the task is claimed. Depends on T30. Done when `fmt` is idempotent on every snapshot case and keeps comments.

Decided with the creator: format through rustfmt: replace the sugar with reversible placeholders that are valid Rust, run rustfmt, then restore the sugar, so the layout matches ordinary Rust and comments survive. Split into subtasks at claim time if it exceeds the task size.

Plan: `rustx-cli fmt <file>` (with `--check`) in a module of its own: a reversible mapping replaces every sugar token sequence with a placeholder that is valid Rust of the same syntactic role and carries a unique id (e.g. `var x` as `let __rustx_fmt_N x`-like forms chosen per sugar item), runs `rustfmt` from the toolchain on the result through stdin, then maps the placeholders back; a sugar item it cannot map leaves the file unformatted with an error, never a changed meaning. Works on `.rustx` files and on `sugar!` bodies inside `.rs` files. Verify: `fmt` is idempotent on every snapshot case, keeps comments, and `expand(fmt(x))` equals `expand(x)` token for token; CLI integration tests; `just check`. If it exceeds the task size, split into T31.1 (mapping for the token-level sugar, CLI) and T31.2 (remaining items).

### T33. M9: user documentation

A user guide: one page per sugar item with the plain Rust it desugars to, the entry points (`sugar!`, the attribute macros, `#[sugared]`, the CLI, `build.rs`) and the limits from ARCHITECTURE h, built from the problem files without duplicating them. Done when the guide builds, every example in it is compiled by a doc test or a `pass/` test, and README links to it.
