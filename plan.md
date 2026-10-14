# cargo-rustx

GitHub: https://github.com/pyrlyn/cargo-rustx

A syntax layer over Rust: sugar desugars into ordinary Rust through procedural macros built on `syn`.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T2 | in progress | P0 | 2 | 0% | Claude / opus-5.5 |
| T3 | in progress | P1 | 4 | 0% | Claude / opus-5.5 |
| T4 | in progress | P1 | 3 | 0% | Claude / opus-5.5 |

### T2. M1: `var` statements

Problem 02. `var x = e;` and `var x: T = e;` (and `var` patterns where `let mut` is valid) desugar to `let mut`. First real token pass: establishes how a pass finds statement position and keeps spans.

Plan: implement `passes/var.rs` on `tokens::map_sequences`; rewrite `var` only at statement start (sequence start, after `;`, after a `}` that ends a statement) when followed by a pattern, so `var` used as an identifier stays untouched. Verify with unit tests, a snapshot case, a `pass/` compile test built from `problems/02-ubiquitous-mut.md`, and a `fail/` case for `var` without a binding.

### T3. M2: type-prefix sugar `~Trait`, `iter T`, `rc T`, `weak T`

Problems 04, 09, 16 (types only; `rc e` expressions and field access belong to M6). `~T` → `Box<dyn T + Send + Sync>`, `iter T` → `impl Iterator<Item = T> + '_`, `rc T` → `Rc<RefCell<T>>`, `weak T` → `Weak<RefCell<T>>`, with fully qualified std paths so user imports do not matter.

Plan: implement `passes/prefix_types.rs`; detect type positions from the preceding token (`:`, `->`, `<`, `,` inside generics, `&`, `(` in tuple types), find the end of the type, rewrite. Verify with unit tests, snapshot cases, `pass/` compile tests from the problem files, and `fail/` cases for a prefix with no type.

### T4. M2: method generics without turbofish

Problem 06. `.m<T>(..)` → `.m::<T>(..)`; `.collect<Vec>()` and other known std collections get their missing arguments as `_` (`Vec<_>`, `HashMap<_, _>`).

Plan: implement `passes/turbofish.rs`; match `.` ident `<` .. `>` `(` with balanced angle brackets, leave existing `::<` and comparisons that do not end in a call group alone. Verify with unit tests, a snapshot case, a `pass/` compile test from the problem file, and a test that `a.b < c` comparisons are unchanged.
