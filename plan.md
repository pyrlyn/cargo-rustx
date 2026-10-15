# cargo-rustx

GitHub: https://github.com/pyrlyn/cargo-rustx

A syntax layer over Rust: sugar desugars into ordinary Rust through procedural macros built on `syn`.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T19 | todo | P2 | 5 | 0% | |
| T19.2 | in progress | P2 | 2 | 0% | Claude / opus-5.5 |
| T24 | in progress | P2 | 4 | 0% | Claude / opus-5.5 |
| T26 | todo | P2 | 4 | 0% | |
| T28 | in progress | P3 | 4 | 0% | Claude / opus-5.5 |
| T29 | todo | P3 | 3 | 0% | |
| T31 | todo | P3 | 3 | 0% | |
| T32 | in progress | P3 | 4 | 0% | Claude / opus-5.5 |
| T33 | todo | P3 | 2 | 0% | |

### T19. Convert plain Rust into rustx sugar

The reverse of `expand`: take existing Rust written without sugar and rewrite it into the rustx form, so old code can adopt the syntax without a manual rewrite. One reverse rule per sugar item that has a pass; a rule fires only when the code is exactly what that pass would generate (e.g. `x: Option<T>` + `unwrap_or(..)` back to `x: T = ..`, `let mut` back to `var`). Anything it cannot prove equivalent stays as written.

Done when: a `rustx-cli sugar <file>` command (or the M9 CLI, if it lands first) prints the converted source; for every reverse rule a test shows `expand(sugar(x))` is token-equal to `x` after normalisation, and valid Rust with no matching shape comes back unchanged; snapshot cases cover each rule. Too big for one task: split into a shared driver plus one subtask per sugar item when claimed.

Split when claimed: T19.1 is the shared driver with the first rule; the other reverse rules become T19.2 onward, one per sugar item, once the driver has landed.

### T19.2. Reverse rule for the turbofish sugar

The first rule on the T19.1 driver after `var`: `.m::<T>(..)` back to `.m<T>(..)` exactly where the turbofish pass would produce it, and nothing that the forward pass would read differently (comparisons in comma lists, see ARCHITECTURE h). Done when the rule is in `sugar::RULES` with `assert_sugar` unit tests, a snapshot case, and valid Rust with no matching shape unchanged.

Plan: `crates/rustx-core/src/sugar/turbofish.rs` walking method calls with explicit generic arguments; the edit removes the `::` before `<`. Verify against `passes/turbofish.rs` which method-call shapes the forward pass accepts; the driver's round-trip check is the safety net, not the rule. Verify: unit tests with `assert_sugar`, `tests/sugar_cases/turbofish.rs` snapshot, `just check`.

### T24. M6: `shared` bindings and `shared T` types

Problem 12. `shared s = e;` becomes a `Shared<T>` binding, `shared T` in type position becomes `Shared<T>`, and field access on such bindings goes through the lock, one lock per access, as the problem file shows; read-only data becomes a plain `Arc`. Depends on T23. The access rewriting is written so T26 can reuse it. Done when the problem's detailed example compiles and runs through `sugar!`, with unit tests, a snapshot case and `pass/` and `fail/` tests.

Plan: token pass `passes/shared.rs::rewrite` (pre-registered after `capture_lists`) turns the statement `shared s = e;` into a binding of `::rustx::Shared::new(e)` marked for the AST pass, and `shared T` in type positions into `::rustx::Shared<T>`; AST pass `shared::lower` (pre-registered before `throws::lower`) rewrites field access on marked bindings to go through `lock()`, giving each statement one guard so two accesses in one statement cannot deadlock, and makes never-mutated data a plain `Arc` as the problem file shows. Verify: unit tests, `tests/cases/shared.rustx`, `pass/shared.rs` running the problem's detailed example, `fail/shared_*.rs`, `just check`.

### T26. M6: borrows on `rc T` field access

Problem 16. On a binding whose type is written `rc T` or `&rc T`, field access gets `.borrow_mut()` when the field is assigned, compound-assigned or receives a known `&mut self` std method call, and `.borrow()` otherwise, plus `with` / `with_mut` helpers in `rustx`. Reuses T24's access rewriting. Depends on T24 and T25. Done when the problem's tree example runs through `sugar!`, with unit tests, a snapshot case and `pass/` and `fail/` tests.

### T28. M8: implicit lossless `into()`

Problem 13. Where the target type is written in the input (a `let` annotation, a field of a struct declared in the input, a parameter of a fn declared in the input), a lossless widening (`u32` to `u64`, `u16` to `u32`, ...) gets `.into()`; lossy conversions stay explicit `as`. Explicit opt-in, as ARCHITECTURE g says; the opt-in form is settled with the creator when the task is claimed. Done when unit tests, a snapshot case and `pass/` tests cover each position, and nothing changes without the opt-in.

Decided with the creator: opt-in through a `#[widen]` attribute on a fn, impl or module, or `#![widen]` at the start of the `sugar!` input; it applies only inside what is marked.

Plan: AST pass `passes/widen.rs::lower` (pre-registered before `throws::lower`) that, inside a fn, impl or module marked `#[widen]` or in a `sugar!` input starting with `#![widen]`, removes the attribute and inserts `.into()` (as `::core::convert::Into::into(..)`) where the source type and the target type are both written in the input and the conversion is a lossless widening from the problem file's table: `let x: T = e`, `s.f = e` / `s.f += e` into a field of a struct declared in the input, and arguments of a fn declared in the input; the source type comes from a `let` annotation, a parameter or a field. Anything not provably widening stays as written. Verify: unit tests, `tests/cases/widen.rustx`, `pass/widen.rs`, `fail/widen_*.rs` for misplaced attributes, `just check`.

### T29. M8: string literals into `String`

Problem 15. A string literal gets `.to_string()` where the target type is written as `String` in the same input (struct fields, `Option<String>`, `Vec<String>`, `let x: String`), on the machinery of T28 and the shared `is_string`. Depends on T28. Done when the problem's `Contact` example compiles, with unit tests, a snapshot case and `pass/` tests, and positions whose type is not written stay unchanged.

### T31. M9: `rustx-cli fmt`

Formatting for sugar sources, so `.rustx` files and `sugar!` bodies get a consistent layout without rustfmt (which rejects the sugar). Scope (round-trip through the parser or a token-level formatter, which constructs) is settled with the creator when the task is claimed. Depends on T30. Done when `fmt` is idempotent on every snapshot case and keeps comments.

Decided with the creator: format through rustfmt: replace the sugar with reversible placeholders that are valid Rust, run rustfmt, then restore the sugar, so the layout matches ordinary Rust and comments survive. Split into subtasks at claim time if it exceeds the task size.

### T32. M9: file-level `.rustx` via `build.rs`

A build helper that desugars `.rustx` files into `OUT_DIR` from `build.rs`, so a whole file can use the sugar and be `include!`d, with `rerun-if-changed` lines and errors that name the `.rustx` file and line. Done when an example crate in the workspace builds from a `.rustx` file and a test shows a sugar error pointing at the right `.rustx` line.

Plan: a new crate `crates/rustx-build` with `rustx_build::expand_file(path)` (and a directory variant) for `build.rs`: it desugars a `.rustx` file into `OUT_DIR`, prints `cargo:rerun-if-changed`, and on error fails the build with `path:line:column: message` naming the `.rustx` file. The expansion and diagnostic code of `rustx-cli` (`expand::expand_source`, `diagnostic::render`) moves into a library both use instead of being copied. An example crate in the workspace builds from a `.rustx` file and `include!`s the output. Verify: unit tests, an integration test on the example crate, a test that a sugar error names the right `.rustx` line, `just check`; `toolchain.md` rows.

### T33. M9: user documentation

A user guide: one page per sugar item with the plain Rust it desugars to, the entry points (`sugar!`, the attribute macros, `#[sugared]`, the CLI, `build.rs`) and the limits from ARCHITECTURE h, built from the problem files without duplicating them. Done when the guide builds, every example in it is compiled by a doc test or a `pass/` test, and README links to it.
