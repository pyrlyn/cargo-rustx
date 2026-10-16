# cargo-rustx

GitHub: https://github.com/pyrlyn/cargo-rustx

A syntax layer over Rust: sugar desugars into ordinary Rust through procedural macros built on `syn`.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T19 | todo | P2 | 5 | 0% | |
| T19.10 | in progress | P2 | 2 | 100% | Claude / opus-5.5 |
| T19.12 | in progress | P2 | 3 | 100% | Claude / opus-5.5 |
| T19.13 | in progress | P2 | 2 | 100% | Claude / opus-5.5 |

### T19. Convert plain Rust into rustx sugar

The reverse of `expand`: take existing Rust written without sugar and rewrite it into the rustx form, so old code can adopt the syntax without a manual rewrite. One reverse rule per sugar item that has a pass; a rule fires only when the code is exactly what that pass would generate (e.g. `x: Option<T>` + `unwrap_or(..)` back to `x: T = ..`, `let mut` back to `var`). Anything it cannot prove equivalent stays as written.

Done when: a `rustx-cli sugar <file>` command (or the M9 CLI, if it lands first) prints the converted source; for every reverse rule a test shows `expand(sugar(x))` is token-equal to `x` after normalisation, and valid Rust with no matching shape comes back unchanged; snapshot cases cover each rule. Too big for one task: split into a shared driver plus one subtask per sugar item when claimed.

Split when claimed: T19.1 is the shared driver with the first rule; the other reverse rules become T19.2 onward, one per sugar item, once the driver has landed.

### T19.10. Reverse rule: `Rc<RefCell<T>>` and `Weak<RefCell<T>>` types back to `rc T` and `weak T`

Inverts the type half of problem 16 (`prefix_types`): a type written `Rc<RefCell<T>>` (any spelling the normaliser equates) becomes `rc T`, and `Weak<RefCell<T>>` becomes `weak T`, in every position where `prefix_types` reads the prefix. Done when field, return, parameter and generic-argument types convert, and positions where the conversion would change the expansion (bindings whose field accesses `rc_borrows` would rewrite, starts of groups that `prefix_types` cannot read) are left alone.

Plan: read `passes/prefix_types.rs` (which positions it reads, the `__rustx_rc` marker) and `passes/rc_borrows.rs` (which bindings it tracks); `sugar/prefix_types.rs` implementing `ReverseRule`; register in `RULES`; README rule list; `assert_sugar` tests and a CLI fixture. Verify with `just check`.

### T19.12. Reverse rule: `impl Default` and `..Default::default()` back to field defaults

Inverts the field half of problem 17 (`field_defaults`): a struct followed by the exact `impl Default` the pass generates becomes the struct with `field: T = e` defaults and no impl, and struct expressions ending in `..Default::default()` for such a struct become a bare `..`. Done when the pass output turns back into the sugar on ordinary code, and an `impl Default` the pass would not generate (other statements, a field not set, a `derive(Default)`) is left alone.

Plan: read `passes/field_defaults.rs` for the emitted shapes (generics, `where`, `cfg`, the `Into::into` on `String` literal defaults); `sugar/field_defaults.rs` implementing `ReverseRule`, the struct edit and the impl removal as one unit (an edit group once T19.11 lands, else one spanning edit); the `..` rewrite of struct expressions only for structs whose defaults are in the same file. Register in `RULES`; README rule list; `assert_sugar` tests and a CLI fixture. Verify with `just check`.

### T19.13. Reverse rule: boxed trait objects and iterator types back to `~Trait` and `iter T`

Inverts the other two type prefixes of `prefix_types` (problems 04 and 09): `Box<dyn Trait + Send + Sync>` becomes `~Trait`, and `impl Iterator<Item = T> + '_` becomes `iter T`, in any spelling the normaliser equates and in the positions `prefix_types` reads. Done when both convert on ordinary code and shapes the pass does not write (other bounds, bound order the pass does not emit, a missing `+ '_`, `dyn Trait` without both markers) are left alone.

Plan: build on T19.10 (`sugar/rc_types.rs`): reuse its position logic and single-candidate expansion check, extending it to the two other prefixes rather than adding a parallel walker; register in `RULES` if it is a separate rule; README rule list; `assert_sugar` tests and a CLI fixture. Verify with `just check`.
