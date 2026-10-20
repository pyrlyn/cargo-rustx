# cargo-rustx

GitHub: https://github.com/pyrlyn/cargo-rustx

A syntax layer over Rust: sugar desugars into ordinary Rust through procedural macros built on `syn`.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T19 | todo | P2 | 5 | 55% | |
| T19.14 | in progress | P2 | 2 | 0% | Claude / opus-5.5 |
| T19.15 | in progress | P2 | 3 | 0% | Claude / opus-5.5 |
| T19.16 | in progress | P2 | 2 | 0% | Claude / opus-5.5 |
| T19.17 | in progress | P2 | 4 | 0% | Claude / opus-5.5 |
| T19.18 | todo | P3 | 3 | 0% | |
| T19.19 | todo | P3 | 3 | 0% | |
| T19.20 | todo | P3 | 4 | 0% | |
| T19.21 | todo | P2 | 3 | 0% | |
| T19.22 | todo | P2 | 2 | 0% | |
| T19.23 | todo | P2 | 4 | 0% | |
| T19.24 | todo | P2 | 3 | 0% | |

### T19. Convert plain Rust into rustx sugar

The reverse of `expand`: take existing Rust written without sugar and rewrite it into the rustx form, so old code can adopt the syntax without a manual rewrite. One reverse rule per sugar item that has a pass; a rule fires only when the code is exactly what that pass would generate (e.g. `x: Option<T>` + `unwrap_or(..)` back to `x: T = ..`, `let mut` back to `var`). Anything it cannot prove equivalent stays as written.

Done when: a `rustx-cli sugar <file>` command (or the M9 CLI, if it lands first) prints the converted source; for every reverse rule a test shows `expand(sugar(x))` is token-equal to `x` after normalisation, and valid Rust with no matching shape comes back unchanged; snapshot cases cover each rule. Too big for one task: split into a shared driver plus one subtask per sugar item when claimed.

Split when claimed: T19.1 is the shared driver with the first rule; the other reverse rules become T19.2 onward, one per sugar item, once the driver has landed.

The driver and the rules T19.2–T19.13 have landed. T19.14–T19.20 are the remaining reverse rules, T19.21–T19.24 the tests, docs, spec and agent skills around the whole conversion. T19 closes when they all have.

Covered without a rule of their own: `call_defaults` (the `param_defaults` rule converts the calls, T19.8); `from_attribute` (only another spelling of `from` variants, T19.17 covers it); `access`, `attr_flags`, `impl_header`, `item_lists`, `macro_args`, `signatures` and `typed_positions` (helpers shared by passes, not sugar items).

Not reversed, because no plain Rust is what the pass generates, so the driver's token check can never accept an edit:

- `shared`: generates `::rustx::Shared`, not `Arc<Mutex<T>>` with `.lock().unwrap()`.
- `throw`: generates `Err(Into::into(e))`; see `sugar/throws.rs`.
- `widen`: generates `<u32 as From<u16>>::from(x)`, not `.into()` or `u32::from(x)`.
- `from_lifetimes`: generates the lifetime `'__rustx_from_<param>`; plain code names its own (`'a`).
- `borrows`: generates the lifetime `'__rustx_borrows`, same reason.
- `derive_presets`: the reverse would have to invent a preset name, a refactoring choice rather than an exact inverse.
- `dyn_traits`: the reverse only matches code that already uses `dynosaur` by hand.

Reversing them needs a driver check that compares up to lifetime renaming and call spelling, which is a separate design question, not part of T19.

### T19.14. Reverse rule: closures after clone lets back to capture lists

`passes/capture_lists.rs` (problem 11) turns `move[a.clone(), b.clone()] || body` into `{ let a = a.clone(); let b = b.clone(); move || body }`, the form people already write by hand. The rule turns such a block back into a capture list.

Done when: `sugar/capture_lists.rs` converts a block made only of `let x = <expr of x>;` statements followed by a `move` closure tail, exactly as the pass builds it; anything else in the block (other statements, a non-`move` closure, a `let` with a pattern or type) stays; unit tests, a `rustx-cli sugar` snapshot case and a line in `docs/guide/capture-lists.md` and `docs/guide/cli.md`.


Plan: add `sugar/capture_lists.rs` and register it in `sugar/mod.rs`; unit tests at the bottom of the module; a fixture in `crates/rustx-cli/tests/fixtures/` with its `rustx-cli sugar` snapshot; update `docs/guide/capture-lists.md` and `docs/guide/cli.md`. Verify with `just check`.
### T19.15. Reverse rule: drop the borrows on `rc` bindings

`passes/rc_borrows.rs` (problem 16) adds `.borrow()` / `.borrow_mut()` on field accesses and value-method calls of a binding typed `rc T`, `&rc T` or bound to an `rc e` value. Today the `rc_values` and `prefix_types` rules keep such a binding plain when the pass would borrow on it (`would_borrow`), so converted code keeps `Rc<RefCell<T>>`. The rule removes exactly the borrows the pass would add back, so the binding can become `rc`.

Done when: a binding converts to `rc` together with its accesses when removing every explicit borrow and expanding gives the original tokens; a binding with a borrow the pass would not add back (a held guard, `try_borrow`, a borrow passed on) stays as today; unit tests, a snapshot case, and `docs/guide/rc.md` and `docs/guide/cli.md` updated.


Plan: in `sugar/rc_values.rs` and `sugar/prefix_types.rs`, replace the `would_borrow` fallback with removing the borrows the pass adds back (shared logic in a new `sugar/rc_borrows.rs`, registered in `sugar/mod.rs` if it edits on its own); unit tests at the bottom of each module; a fixture in `crates/rustx-cli/tests/fixtures/` with its `rustx-cli sugar` snapshot; update `docs/guide/rc.md` and `docs/guide/cli.md`. Verify with `just check`.
### T19.16. Reverse rule: supertrait plus blanket impl back to `bound` presets

`passes/bound_presets.rs` (problem 21) turns `bound Name = A + B;` into a trait with those supertraits and a blanket impl for every type that meets them. The rule turns that pair back into the preset.

Done when: a trait with no items whose only impl is the blanket impl the pass generates converts to `bound`; a trait with items, another impl, generics the pass does not write or a different blanket impl stays; unit tests, a snapshot case, and `docs/guide/bound-presets.md` and `docs/guide/cli.md` updated.


Plan: add `sugar/bound_presets.rs` and register it in `sugar/mod.rs`; unit tests at the bottom of the module; a fixture in `crates/rustx-cli/tests/fixtures/` with its `rustx-cli sugar` snapshot; update `docs/guide/bound-presets.md` and `docs/guide/cli.md`. Verify with `just check`.
### T19.17. Reverse rule: `From`, `Display` and `Error` impls back to `from` variants and `#[display]`

`passes/from_display.rs` (problem 05) turns `Io(from io::Error)` into a `From` impl and `#[display(..)]` into `Display` and `Error` impls. The rule turns hand-written impls of exactly that shape back into the sugar, the way `thiserror` users write it.

Done when: a `From<X> for E` impl whose body is `E::V(e)` for a one-field variant of `X` becomes `V(from X)` and the impl is removed; `Display` and `Error` impls the `#[display]` lowering would generate become the attribute; anything the pass would generate differently (extra statements, other bounds, a second `From` source for the variant) stays; unit tests, a snapshot case, and `docs/guide/from-display.md` and `docs/guide/cli.md` updated.


Plan: add `sugar/from_display.rs` and register it in `sugar/mod.rs`; unit tests at the bottom of the module; a fixture in `crates/rustx-cli/tests/fixtures/` with its `rustx-cli sugar` snapshot; update `docs/guide/from-display.md` and `docs/guide/cli.md`. Verify with `just check`.
### T19.18. Reverse rule: newtype impls back to `#[newtype]`

`passes/newtype.rs` (problem 20) gives a one-field tuple struct `new` and one impl per flag (`deref`, `display`, `from_str`, `validate = f`). The rule turns a struct followed by exactly those impls into the attribute with the matching flags.

Done when: every combination of flags the pass supports converts back, impls the pass would not generate keep the struct as written; unit tests, a snapshot case, and `docs/guide/newtype.md` and `docs/guide/cli.md` updated.

### T19.19. Reverse rule: forwarding impls back to `#[delegate]`

`passes/delegate.rs` (problem 07) gives a struct a trait impl forwarding every method to a field marked `#[delegate(Trait)]`. The rule marks the field and removes an impl of exactly that shape, for the std traits in the pass's table and traits declared in the same file.

Done when: a forwarding impl the pass would generate converts; an impl that forwards only some methods, adapts arguments or results, or names a trait the pass cannot list stays; unit tests, a snapshot case, and `docs/guide/delegate.md` and `docs/guide/cli.md` updated.

### T19.20. Reverse rule: constructor, getters and builder back to `#[data]`

`passes/data.rs` (problem 08) gives a struct with named fields a constructor, getters, optional setters and an optional builder. The rule turns a struct followed by exactly that code into `#[data]` with the matching flags. Lowest value of the rules: hand-written impls rarely match the generated code token for token.

Done when: every flag combination the pass supports converts back, and code that differs in any way (docs, attributes, order, an extra method) stays; unit tests, a snapshot case, and `docs/guide/data.md` and `docs/guide/cli.md` updated.

### T19.21. Round-trip tests over every expansion

Each rule has its own tests; nothing yet checks the conversion as a whole. For every snapshot case in `crates/rustx-core/tests/cases/` and every Rust example in `docs/guide/`, expand it, run `sugar` on the result and expand again: the tokens must equal the first expansion, and `sugar` on the expansion must give back the original sugar wherever a rule covers every item it uses. A file no rule matches must come back byte for byte.

Done when: such a test runs under `just check`, lists the cases it expects to round-trip fully, and fails on any mismatch with the case name and a diff.

### T19.22. User docs: migrating plain Rust to rustx

`docs/guide/cli.md` lists the reverse rules in one long sentence. Users need a page that walks through moving an existing crate: run `rustx-cli sugar`, read the result, check it with `rustx-cli expand` and the crate's tests, and know what stays as written and why.

Done when: `docs/guide/migrate.md` shows a before/after example per reverse rule (examples compiled as doc tests, like the other guide pages), the list of sugar that is never produced by the conversion with the reason (from the T19 card), and a short workflow; `docs/guide/README.md` and `cli.md` link to it, and `cli.md` keeps only the command reference.

### T19.23. Specification of the rustx syntax and the conversion contract

The design lives in `ARCHITECTURE.md` and `problems/`, and the user-facing behaviour in `docs/guide/`, but no single document states normatively what each sugar item accepts, what it expands to, which errors it reports, and what `rustx-cli sugar` guarantees. Agents and tools need one.

Done when: `SPEC.md` at the repo root defines, per sugar item, its grammar, its expansion and its errors, links each to its problem file and guide page, and states the conversion contract (a rule edits only when the result expands to the original tokens; comments and layout outside edits survive byte for byte; the list of items never produced). `ARCHITECTURE.md` and `README.md` link to it instead of restating it. A test checks that every pass in `passes/mod.rs` and every rule in `sugar/mod.rs` has a section, so the spec cannot fall behind.

### T19.24. Agent skills for converting plain Rust to rustx

Agents should be able to move a crate onto rustx on their own. A skill tells them how: when to use `rustx-cli sugar`, how to verify the result (`rustx-cli expand`, `cargo check`, the crate's tests), how to wire `rustx` into the crate (`sugar!`, `#[sugared]` or `.rustx` files via `build.rs`), and which sugar they may add by hand where the conversion stops, using `SPEC.md` and the guide pages as the reference.

Done when: the repo ships the skill(s) in the open Agent Skills format (`skills/<name>/SKILL.md` with frontmatter, plus any scripts they call), the README says how to install them, and a test runs the skill's verification steps on a fixture crate so the instructions cannot rot.
