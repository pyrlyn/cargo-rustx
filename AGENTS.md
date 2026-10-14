# AGENTS.md — cargo-rustx

If an `AGENTS.md` or `CLAUDE.md` exists higher in the tree, follow it too; on conflict, ask the creator.

## Layout

- `crates/rustx-core`: the desugarer. `try_expand` runs token passes, parses a `syn::File`, then runs AST passes.
- `crates/rustx-core/src/passes/`: one module per sugar item. `passes/mod.rs` is the only place that lists passes and their order.
- `crates/rustx-core/src/tokens.rs`: `map_sequences`, the shared token-tree walker. Token passes use it instead of their own recursion.
- `crates/rustx-macros`: the `sugar!` proc macro, a thin shell over `rustx_core::expand`.
- `crates/rustx`: the facade users depend on; re-exports the macros and will hold runtime helpers.
- `problems/`, `ARCHITECTURE.md`, `research.md`: the design. A pass implements exactly what its `problems/NN-*.md` file shows.

## Adding a sugar item

1. Put the pass in its own `passes/<name>.rs` with a `//!` header naming the problem file.
2. Register it in `passes/mod.rs` (`TOKEN_PASSES` if `syn` cannot parse the sugar, `AST_PASSES` if it can).
3. Unit tests at the bottom of the pass module, named as the claim they prove.
4. A snapshot case `crates/rustx-core/tests/cases/<name>.rustx`; accept it with `just bless` and read the `.snap`.
5. A compile test `crates/rustx/tests/pass/<name>.rs` that builds and runs the sugar through `sugar!`, and a `tests/fail/<name>_*.rs` for every misuse the pass reports, with its pinned `.stderr`.
6. Errors: `syn::Error::new(span, ..)` on the offending sugar token. Generated tokens keep the span of the sugar token they replace.
7. Valid Rust must pass through unchanged. A pass that could change the meaning of valid Rust is wrong.

## Gate

`just check` (fmt, clippy with `-D warnings`, nextest, doctests, MSRV) before every commit.

## Tasks

Tasks live in `plan.md` / `todo.md` / `done.md` / `roadmap.md` / `ideas.md`. When several agents work in parallel, only the coordinating agent edits these files; workers report back instead.
