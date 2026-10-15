# cargo-rustx

GitHub: https://github.com/pyrlyn/cargo-rustx

A syntax layer over Rust: sugar desugars into ordinary Rust through procedural macros built on `syn`.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T17 | in progress | P2 | 3 | 0% | Claude / opus-5.5 |
| T18 | todo | P2 | 3 | 0% | |

### T17. M4: `#[newtype]`

Problem 20. `#[newtype(deref, display, from_str, validate = check)] pub struct Email(String);` generates one impl per flag, plus `new` (fallible with `validate`).

Plan: fill the registered stub `passes/newtype.rs` (AST pass, after `from_display`). Parse the flags with `parse_nested_meta` (unknown, repeated and valueless `validate` flags are errors), emit one impl per flag right after the struct, and reuse `from_display`'s format helpers and the `syntax` helpers. Verify with a snapshot case from problem 20, a pass test with asserts and pinned fail tests; `just check` green, no existing snapshot or `.stderr` changed.

### T18. M4: standalone attribute macros

Roadmap M4: `#[data]`, `#[display]`, `#[delegate]`, `#[newtype]` and `#[sugar]` usable as attribute macros on valid Rust items outside `sugar!`, reusing the AST passes of T13–T17 through `rustx-core`. Depends on T14–T17.
