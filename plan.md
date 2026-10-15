# cargo-rustx

GitHub: https://github.com/pyrlyn/cargo-rustx

A syntax layer over Rust: sugar desugars into ordinary Rust through procedural macros built on `syn`.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T12 | in progress | P2 | 2 | 0% | Claude / sonnet-5.5 |
| T13 | in progress | P2 | 3 | 0% | Claude / opus-5.5 |
| T14 | in progress | P2 | 3 | 0% | Claude / opus-5.5 |
| T15 | in progress | P2 | 4 | 0% | Claude / opus-5.5 |
| T16 | in progress | P2 | 4 | 0% | Claude / opus-5.5 |
| T17 | todo | P2 | 3 | 0% | |
| T18 | todo | P2 | 3 | 0% | |

### T12. M4: `derive` presets

Problem 18. `derive Id = Debug, Clone, Copy;` at item position declares a preset; in every `#[derive(..)]` of the same input a preset name expands to its list, deduplicated in order, so presets mix with ordinary derives. A preset that shadows a std derive name is an error.

Plan: own pass `passes/derive_presets.rs`. Token half removes the `derive Name = ..;` items (reusing `tokens::position` like `bound_presets`) and records them as a reserved marker item or attribute the AST half reads; AST half expands `#[derive(..)]` lists on structs, enums and unions (including inside modules). Errors: unknown syntax, empty list, std name shadowing, a preset referring to itself. Unit tests, snapshot case, `pass/` test from the problem file, `fail/` cases.

### T13. M4: field defaults and `S { .., .. }`

Problem 17 (fields). `pub port: u16 = 8080` in a struct generates `impl Default`, and a struct literal ending in a bare `..` becomes `..Default::default()`. Follows RFC 3681 so the pass can switch to the native form once it is stable.

Plan: own pass `passes/field_defaults.rs`. Token half: in a named-field struct body, `field: T = e` becomes a marker attribute; a bare `..` before the closing brace of a struct literal becomes `..::core::default::Default::default()`. AST half generates `impl Default` (fields without a default use `Default::default()`; generics and `where` copied) and removes the markers; a struct with field defaults that also derives `Default` is an error. Unit tests, snapshot case, `pass/` test with the problem's `ServerConfig`, `dev`, `prod`, `fail/` cases.

### T14. M4: `#[data]` constructors and getters

Problem 08. `#[data] struct User { name: String, age: u32 }` generates `new` (all fields), getters returning `&T` (by value for `Copy` primitives) and an optional builder, as the problem file specifies.

Plan: own AST pass `passes/data.rs`, listed before `inline_bounds` and `borrows` so generated impls get the type's generics and lifetime from those passes. Follow the problem file for flags (builder, setters, skipping fields); generics and `where` copied to every impl. Errors: tuple and unit structs if unsupported, unknown flags. Unit tests, snapshot case, `pass/` test from the problem file, `fail/` cases.

### T15. M4: `from` variants and `#[display]`

Problem 05. `enum MyErr { Io(from io::Error) }` generates `From<io::Error> for MyErr`; `#[display(..)]` on an enum or its variants generates `Display` (and `Error` where the problem file asks for it), like thiserror / derive_more.

Plan: own pass `passes/from_display.rs`. Token half marks `from` in single-field tuple variants (`Io(from T)`) with a variant attribute; AST half generates `From` impls and `Display`/`Error` per the problem file's detailed example. Errors: `from` on a multi-field variant, two variants `from` the same type, a format string naming a missing field. Unit tests, snapshot case, `pass/` test from the problem file, `fail/` cases.

### T16. M4: `#[delegate]` fields

Problem 07. `#[delegate(Read)] inner: File` generates `impl Read for Wrapper` forwarding every method to the field.

Plan: own AST pass `passes/delegate.rs` with a built-in table of std trait method signatures (`Read`, `Write`, `Iterator`, `Display`, `Debug`, `Deref`-free list per the problem file), plus traits declared in the same input; other traits need the methods listed explicitly. Errors: unknown trait without a method list, a field on an enum, two fields delegating the same trait. Unit tests, snapshot case, `pass/` test from the problem file, `fail/` cases.

### T17. M4: `#[newtype]`

Problem 20. `#[newtype(deref, display, from_str, validate = check)] pub struct Email(String);` generates one impl per flag, plus `new` (fallible with `validate`).

### T18. M4: standalone attribute macros

Roadmap M4: `#[data]`, `#[display]`, `#[delegate]`, `#[newtype]` and `#[sugar]` usable as attribute macros on valid Rust items outside `sugar!`, reusing the AST passes of T13–T17 through `rustx-core`. Depends on T14–T17.
