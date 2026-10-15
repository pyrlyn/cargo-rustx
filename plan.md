# cargo-rustx

GitHub: https://github.com/pyrlyn/cargo-rustx

A syntax layer over Rust: sugar desugars into ordinary Rust through procedural macros built on `syn`.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T18 | in progress | P2 | 3 | 0% | Claude / opus-5.5 |

### T18. M4: standalone attribute macros

Roadmap M4: `#[data]`, `#[display]`, `#[delegate]`, `#[newtype]` and `#[sugar]` usable as attribute macros on valid Rust items outside `sugar!`, reusing the AST passes of T13–T17 through `rustx-core`. Depends on T14–T17.

Plan: a public `rustx-core` entry point that rebuilds the attribute plus the item into a `syn::File` and runs the named AST pass (`#[sugar]`: every AST pass), returning errors as `compile_error!`; thin `#[proc_macro_attribute]` shells in `rustx-macros` re-exported from `rustx`. `#[data]`, `#[display]` (with variant `#[from]`), `#[delegate]` (on the struct, fields keep `#[delegate(..)]`) and `#[sugar]` first; `#[newtype]` is wired after T17 lands. Verify with unit tests in the core, pass tests that use the attributes outside `sugar!`, and pinned fail tests; `just check` green, no existing snapshot or `.stderr` changed.
