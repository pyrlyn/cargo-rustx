# cargo-rustx

GitHub: https://github.com/pyrlyn/cargo-rustx

A syntax layer over Rust: sugar desugars into ordinary Rust through procedural macros built on `syn`.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T19 | todo | P2 | 5 | 0% | |
| T19.5 | in progress | P2 | 2 | 100% | Claude / opus-5.5 |
| T19.6 | in progress | P2 | 3 | 100% | Claude / opus-5.5 |
| T19.7 | in progress | P2 | 4 | 100% | Claude / opus-5.5 |
| T19.8 | in progress | P2 | 3 | 100% | Claude / opus-5.5 |
| T19.9 | in progress | P2 | 3 | 100% | Claude / opus-5.5 |
| T19.10 | in progress | P2 | 2 | 100% | Claude / opus-5.5 |
| T19.11 | in progress | P2 | 3 | 100% | Claude / opus-5.5 |
| T19.12 | in progress | P2 | 3 | 100% | Claude / opus-5.5 |
| T19.13 | in progress | P2 | 2 | 100% | Claude / opus-5.5 |
| T31.2 | in progress | P3 | 3 | 100% | Claude / opus-5.5 |
| T33 | in progress | P3 | 2 | 100% | Claude / opus-5.5 |

### T19. Convert plain Rust into rustx sugar

The reverse of `expand`: take existing Rust written without sugar and rewrite it into the rustx form, so old code can adopt the syntax without a manual rewrite. One reverse rule per sugar item that has a pass; a rule fires only when the code is exactly what that pass would generate (e.g. `x: Option<T>` + `unwrap_or(..)` back to `x: T = ..`, `let mut` back to `var`). Anything it cannot prove equivalent stays as written.

Done when: a `rustx-cli sugar <file>` command (or the M9 CLI, if it lands first) prints the converted source; for every reverse rule a test shows `expand(sugar(x))` is token-equal to `x` after normalisation, and valid Rust with no matching shape comes back unchanged; snapshot cases cover each rule. Too big for one task: split into a shared driver plus one subtask per sugar item when claimed.

Split when claimed: T19.1 is the shared driver with the first rule; the other reverse rules become T19.2 onward, one per sugar item, once the driver has landed.

### T19.5. Reverse rule: string literal conversions back to a bare literal

Inverts T29: `String::from("lit")`, `"lit".to_string()`, `"lit".to_owned()` and `"lit".into()` where the position is written `String` (and the `Some(..)` / `vec![..]` shapes of `Option<String>` / `Vec<String>`) become the bare literal, because `string_literals` turns it back into `String::from`. Done when the rule fires in every position T29 converts and nowhere else.

Plan: `sugar/string_literals.rs` implementing `ReverseRule`, using `typed_positions` (or the same position knowledge) to find typed places; only edits whose expansion the driver check accepts survive, so the rule may propose broadly but tests pin where it fires and where it must not (untyped `let`, `&str` positions, method arguments, `+=`). Register in `RULES`; README rule list; `assert_sugar` tests and a CLI fixture. Verify with `just check`.

### T19.6. Reverse rule: `impl Into<T>` parameters back to `into T`

Inverts the `into T` parameter pass (problem 15): a parameter `x: impl Into<T>` whose body starts with the exact rebinding the pass emits becomes `x: into T` with the rebinding removed. Done when the rule turns the pass output back into the sugar on ordinary code and leaves any other use of `impl Into<T>` alone.

Plan: read `passes/into_params.rs` for the exact emitted shape (rebinding statement, generic or `impl` form); `sugar/into_params.rs` implementing `ReverseRule` with two edits per parameter (the type and the rebinding), both kept or both dropped; register in `RULES`; README rule list; `assert_sugar` tests and a CLI fixture. Verify with `just check`.

### T19.7. Reverse rule: `Result` functions back to `throws`

Inverts T20 for the explicit form: a fn returning `Result<T, E>` whose every tail and `return` value the `throws` pass would have wrapped (`Ok(..)` around values, `Err(..)`/`?` kept) becomes `-> T throws E` with the wrappers removed. Done when the pass output turns back into the sugar on ordinary code and a fn the pass would not produce (a tail that is already a `Result` expression, `Ok` used as a value elsewhere in a way the pass would not emit) is left alone.

Plan: read `passes/throws.rs` (and `throw.rs` for `return Err(..)` back to `throw e` if it fits the size) for the exact emitted shapes; `sugar/throws.rs` implementing `ReverseRule`, all edits of one fn kept or dropped together; the anyhow default form (`throws` without `E`) is out of scope unless the normaliser already equates its path. Register in `RULES`; README rule list; `assert_sugar` tests and a CLI fixture. Verify with `just check`.

### T19.8. Reverse rule: `Option` parameters with `unwrap_or` back to parameter defaults

Inverts parameter defaults (problem 17, from the T19 card): `x: Option<T>` plus the exact `let x = x.unwrap_or(..)` (or whatever `param_defaults` emits) becomes `x: T = ..`. Call sites are out of scope unless the pass rewrites them in a way a rule can invert safely. Done when the pass output turns back into the sugar and other `Option` parameters are left alone.

Plan: read `passes/param_defaults.rs` and `call_defaults` for the emitted shapes; `sugar/param_defaults.rs` implementing `ReverseRule` with the type and rebinding edits kept or dropped together; register in `RULES`; README rule list; `assert_sugar` tests and a CLI fixture. Verify with `just check`.

### T19.9. Reverse rule: generic bounds back to inline bounds

Inverts inline bounds (problem 3): a generic parameter used by exactly one fn parameter, declared with bounds the `inline_bounds` pass would have generated, becomes `x: T: Bound` with the generic removed. Done when the pass output turns back into the sugar and generics used elsewhere (return type, other parameters, body turbofish, where clauses the pass would not write) are left alone.

Plan: read `passes/inline_bounds.rs` for the emitted shape (generic list position, naming, where clauses); `sugar/inline_bounds.rs` implementing `ReverseRule` with the generic removal and the parameter edit kept or dropped together; register in `RULES`; README rule list; `assert_sugar` tests and a CLI fixture. Verify with `just check`.

### T19.10. Reverse rule: `Rc<RefCell<T>>` and `Weak<RefCell<T>>` types back to `rc T` and `weak T`

Inverts the type half of problem 16 (`prefix_types`): a type written `Rc<RefCell<T>>` (any spelling the normaliser equates) becomes `rc T`, and `Weak<RefCell<T>>` becomes `weak T`, in every position where `prefix_types` reads the prefix. Done when field, return, parameter and generic-argument types convert, and positions where the conversion would change the expansion (bindings whose field accesses `rc_borrows` would rewrite, starts of groups that `prefix_types` cannot read) are left alone.

Plan: read `passes/prefix_types.rs` (which positions it reads, the `__rustx_rc` marker) and `passes/rc_borrows.rs` (which bindings it tracks); `sugar/prefix_types.rs` implementing `ReverseRule`; register in `RULES`; README rule list; `assert_sugar` tests and a CLI fixture. Verify with `just check`.

### T19.11. Sugar driver: trailing commas and edit groups

Two limits found by T19.6 and T19.9. First, code formatted by rustfmt never converts when a rule spans a list rustfmt ends with a trailing comma (a multi-line `where`, generics, parameters), because the check compares tokens exactly; a trailing comma before a closing delimiter or before the body brace of a `where` list never changes meaning outside macro calls, so the check should ignore it there (never inside a macro invocation's tokens, never the comma of a one-element tuple). Second, the driver has no edit groups, so multi-place rules (T19.6, T19.9) emit one edit spanning the whole signature, and other rules' edits inside that span are dropped. Done when the check accepts rustfmt's trailing commas, the driver keeps or drops a group of edits as one unit (including in its one-at-a-time fallback), and a test shows `into T` and inline bounds converting in the same fn.

Plan: in `crates/rustx-core/src/sugar/mod.rs`, extend the token comparison (next to the `std_paths` normaliser) with the trailing comma rule above, and add an edit group to `ReverseRule` output (e.g. `Vec<Vec<Edit>>` or an `Edit` group id) with the fallback adding whole groups. Keep the existing rules working unchanged; moving T19.6/T19.9 onto groups is done when their branches merge, not here. Edit groups already landed with T19.7 (`ReverseRule::edit_groups`, commit `be71ac6`): merge that commit and build on it instead of a second mechanism. Also fix a `std_paths` gap T19.7 found: enum variants are not recorded as declarations, so `use self::Outcome::Ok;` does not shadow the prelude `Ok`. Verify: unit tests for the comparison (where, generics, params, 1-tuple, macro tokens), a failing-first test for the enum variant gap, `just check`.

### T19.12. Reverse rule: `impl Default` and `..Default::default()` back to field defaults

Inverts the field half of problem 17 (`field_defaults`): a struct followed by the exact `impl Default` the pass generates becomes the struct with `field: T = e` defaults and no impl, and struct expressions ending in `..Default::default()` for such a struct become a bare `..`. Done when the pass output turns back into the sugar on ordinary code, and an `impl Default` the pass would not generate (other statements, a field not set, a `derive(Default)`) is left alone.

Plan: read `passes/field_defaults.rs` for the emitted shapes (generics, `where`, `cfg`, the `Into::into` on `String` literal defaults); `sugar/field_defaults.rs` implementing `ReverseRule`, the struct edit and the impl removal as one unit (an edit group once T19.11 lands, else one spanning edit); the `..` rewrite of struct expressions only for structs whose defaults are in the same file. Register in `RULES`; README rule list; `assert_sugar` tests and a CLI fixture. Verify with `just check`.

### T19.13. Reverse rule: boxed trait objects and iterator types back to `~Trait` and `iter T`

Inverts the other two type prefixes of `prefix_types` (problems 04 and 09): `Box<dyn Trait + Send + Sync>` becomes `~Trait`, and `impl Iterator<Item = T> + '_` becomes `iter T`, in any spelling the normaliser equates and in the positions `prefix_types` reads. Done when both convert on ordinary code and shapes the pass does not write (other bounds, bound order the pass does not emit, a missing `+ '_`, `dyn Trait` without both markers) are left alone.

Plan: build on T19.10 (`sugar/rc_types.rs`): reuse its position logic and single-candidate expansion check, extending it to the two other prefixes rather than adding a parallel walker; register in `RULES` if it is a separate rule; README rule list; `assert_sugar` tests and a CLI fixture. Verify with `just check`.

### T31.2. M9: `rustx-cli fmt` for the remaining sugar

The rest of T31 after T31.1: placeholders for `bound`/`derive` presets, inline bounds `x: T: Bound`, parameter defaults `x: T = v`, call defaults and named arguments `f(name: v)`, capture lists `move[..] |x|`, and formatting `sugar!` bodies inside `.rs` files. Done when the `NOT_YET` list in `crates/rustx-cli/tests/fmt.rs` holds only the cases that do not expand, and `.rs` files with `sugar!` bodies format with the same guarantees as `.rustx` files.

Plan: extend `crates/rustx-cli/src/fmt/find.rs` with the four remaining spellings (presets as an item kept verbatim behind an `<id>!();` macro-call item; inline bounds and parameter defaults as a `#[<id>]` attribute on the parameter with the suffix cut and restored at the parameter end; named and default call arguments as `<id>name = v` only after a non-`fn` ident; capture lists as `move |<id>, x|`), porting the working draft from T31.1 onto the current code instead of copying it. Then format `sugar! { .. }` bodies in `.rs` files: find each invocation, format its body as a `.rustx` source at the invocation's indentation, and splice it back; the rest of the file goes through plain rustfmt only if the user asks (not by default). Verify: unit tests per spelling, the integration test with a shrunk `NOT_YET` list, a `.rs` fixture with two `sugar!` bodies, `just check`. Split again if it exceeds the task size.

### T33. M9: user documentation

A user guide: one page per sugar item with the plain Rust it desugars to, the entry points (`sugar!`, the attribute macros, `#[sugared]`, the CLI, `build.rs`) and the limits from ARCHITECTURE h, built from the problem files without duplicating them. Done when the guide builds, every example in it is compiled by a doc test or a `pass/` test, and README links to it.

Plan: an mdBook-free guide in `docs/guide/` (plain Markdown, rendered by GitHub): `README.md` index, one page per sugar item (what it is, a short sugar example, the plain Rust it becomes, its limits from ARCHITECTURE h, a link to the problem file instead of repeating its motivation), and pages for the entry points (`sugar!`, attribute macros, `#[sugared]`, `rustx-cli expand`/`sugar`, `build.rs` via `rustx-build`). Every example is compiled: each page is included into a `rustx` doc-test module through `#[doc = include_str!(..)]` under `cfg(doctest)`, so `rust` blocks run as doc tests (expansions that are not standalone use `ignore` only when the same code is already a `pass/` test, named next to the block). README links to the guide. Also fixes the stale "string literals into `String` fields are M8" comments in `tests/pass/into_params.rs` and `tests/cases/into_params.rustx` (re-blessed; only the comment line changes). The `rustx-cli fmt` page is added when T31 merges. Verify: `just check` runs every guide example; every sugar item in `passes/mod.rs` has a page.
