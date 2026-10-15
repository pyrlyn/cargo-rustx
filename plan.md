# cargo-rustx

GitHub: https://github.com/pyrlyn/cargo-rustx

A syntax layer over Rust: sugar desugars into ordinary Rust through procedural macros built on `syn`.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T19 | todo | P2 | 5 | 0% | |
| T20 | in progress | P2 | 4 | 0% | Claude / opus-5.5 |
| T21 | todo | P2 | 2 | 0% | |
| T22 | in progress | P2 | 3 | 0% | Claude / opus-5.5 |
| T23 | in progress | P2 | 2 | 0% | Claude / opus-5.5 |
| T24 | todo | P2 | 4 | 0% | |
| T25 | in progress | P2 | 2 | 0% | Claude / opus-5.5 |
| T26 | todo | P2 | 4 | 0% | |
| T27 | todo | P2 | 4 | 0% | |
| T28 | todo | P3 | 4 | 0% | |
| T29 | todo | P3 | 3 | 0% | |
| T30 | in progress | P2 | 3 | 0% | Claude / opus-5.5 |
| T31 | todo | P3 | 3 | 0% | |
| T32 | todo | P3 | 4 | 0% | |
| T33 | todo | P3 | 2 | 0% | |

### T19. Convert plain Rust into rustx sugar

The reverse of `expand`: take existing Rust written without sugar and rewrite it into the rustx form, so old code can adopt the syntax without a manual rewrite. One reverse rule per sugar item that has a pass; a rule fires only when the code is exactly what that pass would generate (e.g. `x: Option<T>` + `unwrap_or(..)` back to `x: T = ..`, `let mut` back to `var`). Anything it cannot prove equivalent stays as written.

Done when: a `rustx-cli sugar <file>` command (or the M9 CLI, if it lands first) prints the converted source; for every reverse rule a test shows `expand(sugar(x))` is token-equal to `x` after normalisation, and valid Rust with no matching shape comes back unchanged; snapshot cases cover each rule. Too big for one task: split into a shared driver plus one subtask per sugar item when claimed.

### T20. M5: `T throws` return types and tail `Ok(..)`

Problem 10. `fn load() -> Config throws` returns `anyhow::Result<Config>` and its tail expression is wrapped in `Ok(..)`. `anyhow` is reached through a re-export in the `rustx` facade (the `throws` result alias of ARCHITECTURE b), so users do not add it themselves. Done when the problem's detailed example expands to its "As is" code apart from the error type, with unit tests, a snapshot case, `pass/` and `fail/` tests; which tails are left alone (one that is already `Ok(..)`/`Err(..)`, `?`-only bodies, a body ending in `return`) is settled with the creator when the task is claimed.

Decided with the creator: `T throws` returns `anyhow::Result<T>` (through the `rustx` re-export) and `T throws E` returns `Result<T, E>`. The tail is wrapped in `Ok(..)` unless it already is `Ok(..)` or `Err(..)`, or does not produce a value (`return`, `panic!`, `unreachable!`, `todo!`, `unimplemented!`, `loop`); a body without a tail gets `Ok(())`.

Plan: token pass `passes/throws.rs::rewrite` (pre-registered before `prefix_types`, so `iter T throws` becomes `Result<iter T>` and `prefix_types` still sees the type) turns `-> T throws [E]` into the `Result` type and leaves a marker attribute on the fn; AST pass `throws::lower` (pre-registered last in `AST_PASSES`) wraps the tail of marked fns and removes the marker. `rustx` re-exports `anyhow`. Applies to fns, methods and trait methods with bodies; closures and nested fns keep their own signature. Verify: unit tests, `tests/cases/throws.rustx`, `pass/throws.rs`, `fail/throws_*.rs`, `just check`.

### T21. M5: `throw e`

Problem 10. `throw e` inside a `throws` fn becomes `return Err(::core::convert::Into::into(e))`; outside one it is an error at `throw`. Depends on T20. Done when unit tests, a snapshot case and `pass/` and `fail/` tests cover it, including `throw` inside closures and nested fns, which belong to their own signature.

### T22. M6: `move[..]` capture lists

Problem 11. `move[a.clone(), b.clone()] || work(a, b)` becomes `{ let a = a.clone(); let b = b.clone(); move || work(a, b) }`, keeping the user's names. The form stays close to `move(expr)` (RFC 3968) so the desugaring can switch to native syntax later without changing user code. Done when unit tests, a snapshot case and `pass/` and `fail/` tests cover plain and `async` closures and captures that are not a place expression.

Plan: token pass `passes/capture_lists.rs` (pre-registered in `TOKEN_PASSES` after `var`) that turns `move[e1, e2] <closure>` into a block of `let` bindings plus `move <closure>`, naming each binding after the place the capture starts with; `move[` is never valid Rust, so nothing else changes. Errors on an empty list, a capture with no name to take, and `move[..]` not followed by a closure or `async` block. Verify: unit tests, `tests/cases/capture_lists.rustx`, `pass/capture_lists.rs`, `fail/capture_lists_*.rs`, `just check`.

### T23. M6: `Shared<T>` runtime type

Problem 12. A `Shared<T>` type in the `rustx` facade wrapping `Arc<Mutex<T>>`, with `Shared::new`, `Clone`, a lock accessor the `shared` sugar desugars to, and `with(|v| ..)` / `with_mut(|v| ..)` for an atomic group of operations. The `rustx` crate's first runtime code. Done when its API has doc tests and unit tests, including poisoning behaviour.

Plan: `crates/rustx/src/shared.rs` with `Shared<T>` over `Arc<Mutex<T>>`: `new`, `Clone`, `Default`, `Debug`, `From<T>`, a `lock()` returning the guard (recovering from poisoning, documented), `with(|&T| ..)` and `with_mut(|&mut T| ..)`; re-exported from `rustx`. Verify: unit tests (clones share the value, threads, poisoning), doc tests on every public item, `just check`.

### T24. M6: `shared` bindings and `shared T` types

Problem 12. `shared s = e;` becomes a `Shared<T>` binding, `shared T` in type position becomes `Shared<T>`, and field access on such bindings goes through the lock, one lock per access, as the problem file shows; read-only data becomes a plain `Arc`. Depends on T23. The access rewriting is written so T26 can reuse it. Done when the problem's detailed example compiles and runs through `sugar!`, with unit tests, a snapshot case and `pass/` and `fail/` tests.

### T25. M6: `rc e` expressions

Problem 16. `rc e` in expression position becomes `Rc::new(RefCell::new(e))` (the `rc T` / `weak T` types landed in M2). Expression position is told apart from type position as ARCHITECTURE h describes. Done when unit tests, a snapshot case and `pass/` tests cover arguments, `let` values and struct field values, and valid Rust using `rc` as an identifier still passes through unchanged.

Plan: token pass `passes/rc_values.rs` (pre-registered in `TOKEN_PASSES` right after `prefix_types`, so every `rc` left is outside a type position) that rewrites `rc <expr>` into `::std::rc::Rc::new(::std::cell::RefCell::new(<expr>))` only where `rc` is a whole identifier followed by an expression start that would not be valid Rust after it (`rc(x)`, `rc[0]`, `rc.f`, `rc { .. }` stay). The expression extends to the next `,` or `;` at the same depth or the end of its group. Verify: unit tests including pass-through of `rc` as an identifier, `tests/cases/rc_values.rustx`, `pass/rc_values.rs`, `just check`.

### T26. M6: borrows on `rc T` field access

Problem 16. On a binding whose type is written `rc T` or `&rc T`, field access gets `.borrow_mut()` when the field is assigned, compound-assigned or receives a known `&mut self` std method call, and `.borrow()` otherwise, plus `with` / `with_mut` helpers in `rustx`. Reuses T24's access rewriting. Depends on T24 and T25. Done when the problem's tree example runs through `sugar!`, with unit tests, a snapshot case and `pass/` and `fail/` tests.

### T27. M7: `async fn` in traits for `dyn` use

Problem 14. A trait with `async fn` stays native when it is not used as `dyn`; for `dyn` use the macro generates code for the maintained `dynosaur` crate instead of hand-rolled boxing (research.md). How a trait is marked for `dyn` use is settled with the creator when the task is claimed. Done when the problem's detailed example compiles, including a `dyn` call, with unit tests, a snapshot case and `pass/` and `fail/` tests; split into subtasks at claim time if it exceeds the task size.

Decided with the creator: a trait is marked for `dyn` use by an attribute on the trait; without it the trait stays native.

### T28. M8: implicit lossless `into()`

Problem 13. Where the target type is written in the input (a `let` annotation, a field of a struct declared in the input, a parameter of a fn declared in the input), a lossless widening (`u32` to `u64`, `u16` to `u32`, ...) gets `.into()`; lossy conversions stay explicit `as`. Explicit opt-in, as ARCHITECTURE g says; the opt-in form is settled with the creator when the task is claimed. Done when unit tests, a snapshot case and `pass/` tests cover each position, and nothing changes without the opt-in.

Decided with the creator: opt-in through a `#[widen]` attribute on a fn, impl or module, or `#![widen]` at the start of the `sugar!` input; it applies only inside what is marked.

### T29. M8: string literals into `String`

Problem 15. A string literal gets `.to_string()` where the target type is written as `String` in the same input (struct fields, `Option<String>`, `Vec<String>`, `let x: String`), on the machinery of T28 and the shared `is_string`. Depends on T28. Done when the problem's `Contact` example compiles, with unit tests, a snapshot case and `pass/` tests, and positions whose type is not written stay unchanged.

### T30. M9: `rustx-cli expand`

A `rustx-cli` binary whose `expand <file>` prints the plain Rust a `sugar!` body or `.rustx` file desugars to, formatted with `prettyplease`, and reports errors with file positions. T19 builds its reverse command on the same CLI. Done when integration tests run the binary on the problem examples and compare with snapshots.

Plan: new workspace crate `crates/rustx-cli` (binary `rustx-cli`, `clap` derive) with `expand <file>`: reads a `.rustx` file or a Rust file, desugars every `sugar! { .. }` body (or the whole file for `.rustx`) through `rustx_core::expand`, prints it formatted with `prettyplease`, and on error prints `file:line:column: message` and exits non-zero. Verify: integration tests running the binary on problem examples with `insta` snapshots and an error case, `just check`; `toolchain.md` rows for new crates.

### T31. M9: `rustx-cli fmt`

Formatting for sugar sources, so `.rustx` files and `sugar!` bodies get a consistent layout without rustfmt (which rejects the sugar). Scope (round-trip through the parser or a token-level formatter, which constructs) is settled with the creator when the task is claimed. Depends on T30. Done when `fmt` is idempotent on every snapshot case and keeps comments.

### T32. M9: file-level `.rustx` via `build.rs`

A build helper that desugars `.rustx` files into `OUT_DIR` from `build.rs`, so a whole file can use the sugar and be `include!`d, with `rerun-if-changed` lines and errors that name the `.rustx` file and line. Done when an example crate in the workspace builds from a `.rustx` file and a test shows a sugar error pointing at the right `.rustx` line.

### T33. M9: user documentation

A user guide: one page per sugar item with the plain Rust it desugars to, the entry points (`sugar!`, the attribute macros, `#[sugared]`, the CLI, `build.rs`) and the limits from ARCHITECTURE h, built from the problem files without duplicating them. Done when the guide builds, every example in it is compiled by a doc test or a `pass/` test, and README links to it.
