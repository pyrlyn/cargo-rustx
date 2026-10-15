# cargo-rustx

GitHub: https://github.com/pyrlyn/cargo-rustx

A syntax layer over Rust: sugar desugars into ordinary Rust through procedural macros built on `syn`.

| # | Status | Priority | Complexity | Readiness | Agent |
| --- | --- | --- | --- | --- | --- |
| T19 | todo | P2 | 5 | 0% | |
| T19.3 | in progress | P2 | 3 | 100% | Claude / opus-5.5 |
| T19.4 | in progress | P2 | 2 | 100% | Claude / opus-5.5 |
| T19.5 | in progress | P2 | 2 | 0% | Claude / opus-5.5 |
| T19.6 | in progress | P2 | 3 | 0% | Claude / opus-5.5 |
| T31.2 | in progress | P3 | 3 | 100% | Claude / opus-5.5 |
| T33 | in progress | P3 | 2 | 100% | Claude / opus-5.5 |

### T19. Convert plain Rust into rustx sugar

The reverse of `expand`: take existing Rust written without sugar and rewrite it into the rustx form, so old code can adopt the syntax without a manual rewrite. One reverse rule per sugar item that has a pass; a rule fires only when the code is exactly what that pass would generate (e.g. `x: Option<T>` + `unwrap_or(..)` back to `x: T = ..`, `let mut` back to `var`). Anything it cannot prove equivalent stays as written.

Done when: a `rustx-cli sugar <file>` command (or the M9 CLI, if it lands first) prints the converted source; for every reverse rule a test shows `expand(sugar(x))` is token-equal to `x` after normalisation, and valid Rust with no matching shape comes back unchanged; snapshot cases cover each rule. Too big for one task: split into a shared driver plus one subtask per sugar item when claimed.

Split when claimed: T19.1 is the shared driver with the first rule; the other reverse rules become T19.2 onward, one per sugar item, once the driver has landed.

### T19.3. Std path equivalence in the sugar check

Passes emit fully qualified std paths (`::std::rc::Rc::new`, `::core::option::Option`), so the T19.1 check `expand(sugar(x)) == x` never matches ordinary code. Decided with the creator: the check treats a short std name (`Rc`, `RefCell`, `Option`, `Some`, `String`, `Vec`, `Box`, `Arc`, `Mutex`, `Result`, `Ok`, `Err`, `Into`, `From`, `Default`, ...) as equal to its fully qualified path when the file declares no item or `use` that shadows that name. Done when the shared driver applies this normalisation and a test shows a shadowing `use`/item turns it off. Split at implementation time because the whole exceeded the task size: the first rule that needs it is T19.4.

Plan: in `crates/rustx-core/src/sugar/`, add a normaliser used only by the equivalence check (never by rewriting): it rewrites known std paths in both token streams to one canonical spelling, after collecting the names the file shadows (items, `use` trees, generic parameters). The rule `sugar/rc_values.rs` implements `ReverseRule` for `Rc::new(RefCell::new(e))` in any of its spellings, but only where the result type is not written by hand in a way `rc e` cannot express. Register it in `sugar::RULES`. Verify: unit tests for the normaliser (equal, shadowed, partially qualified), `assert_sugar` cases for the rule, a `rustx-cli sugar` snapshot, `just check`.

### T19.4. Reverse rule: `Rc::new(RefCell::new(e))` back to `rc e`

The first reverse rule that needs T19.3: `Rc::new(RefCell::new(e))` in any spelling the normaliser equates becomes `rc e` in ordinary code. Done when the rule fires on ordinary code, refuses positions where `rc e` reads differently, and the driver check refuses bindings whose field accesses `rc_borrows` would rewrite.

Plan: `sugar/rc_values.rs` implementing `ReverseRule`, registered in `sugar::RULES`; `assert_sugar` unit tests; a `rustx-cli sugar` fixture and snapshot; README rule list. Verify with `just check`.

### T19.5. Reverse rule: string literal conversions back to a bare literal

Inverts T29: `String::from("lit")`, `"lit".to_string()`, `"lit".to_owned()` and `"lit".into()` where the position is written `String` (and the `Some(..)` / `vec![..]` shapes of `Option<String>` / `Vec<String>`) become the bare literal, because `string_literals` turns it back into `String::from`. Done when the rule fires in every position T29 converts and nowhere else.

Plan: `sugar/string_literals.rs` implementing `ReverseRule`, using `typed_positions` (or the same position knowledge) to find typed places; only edits whose expansion the driver check accepts survive, so the rule may propose broadly but tests pin where it fires and where it must not (untyped `let`, `&str` positions, method arguments, `+=`). Register in `RULES`; README rule list; `assert_sugar` tests and a CLI fixture. Verify with `just check`.

### T19.6. Reverse rule: `impl Into<T>` parameters back to `into T`

Inverts the `into T` parameter pass (problem 15): a parameter `x: impl Into<T>` whose body starts with the exact rebinding the pass emits becomes `x: into T` with the rebinding removed. Done when the rule turns the pass output back into the sugar on ordinary code and leaves any other use of `impl Into<T>` alone.

Plan: read `passes/into_params.rs` for the exact emitted shape (rebinding statement, generic or `impl` form); `sugar/into_params.rs` implementing `ReverseRule` with two edits per parameter (the type and the rebinding), both kept or both dropped; register in `RULES`; README rule list; `assert_sugar` tests and a CLI fixture. Verify with `just check`.

### T31.2. M9: `rustx-cli fmt` for the remaining sugar

The rest of T31 after T31.1: placeholders for `bound`/`derive` presets, inline bounds `x: T: Bound`, parameter defaults `x: T = v`, call defaults and named arguments `f(name: v)`, capture lists `move[..] |x|`, and formatting `sugar!` bodies inside `.rs` files. Done when the `NOT_YET` list in `crates/rustx-cli/tests/fmt.rs` holds only the cases that do not expand, and `.rs` files with `sugar!` bodies format with the same guarantees as `.rustx` files.

Plan: extend `crates/rustx-cli/src/fmt/find.rs` with the four remaining spellings (presets as an item kept verbatim behind an `<id>!();` macro-call item; inline bounds and parameter defaults as a `#[<id>]` attribute on the parameter with the suffix cut and restored at the parameter end; named and default call arguments as `<id>name = v` only after a non-`fn` ident; capture lists as `move |<id>, x|`), porting the working draft from T31.1 onto the current code instead of copying it. Then format `sugar! { .. }` bodies in `.rs` files: find each invocation, format its body as a `.rustx` source at the invocation's indentation, and splice it back; the rest of the file goes through plain rustfmt only if the user asks (not by default). Verify: unit tests per spelling, the integration test with a shrunk `NOT_YET` list, a `.rs` fixture with two `sugar!` bodies, `just check`. Split again if it exceeds the task size.

### T33. M9: user documentation

A user guide: one page per sugar item with the plain Rust it desugars to, the entry points (`sugar!`, the attribute macros, `#[sugared]`, the CLI, `build.rs`) and the limits from ARCHITECTURE h, built from the problem files without duplicating them. Done when the guide builds, every example in it is compiled by a doc test or a `pass/` test, and README links to it.

Plan: an mdBook-free guide in `docs/guide/` (plain Markdown, rendered by GitHub): `README.md` index, one page per sugar item (what it is, a short sugar example, the plain Rust it becomes, its limits from ARCHITECTURE h, a link to the problem file instead of repeating its motivation), and pages for the entry points (`sugar!`, attribute macros, `#[sugared]`, `rustx-cli expand`/`sugar`, `build.rs` via `rustx-build`). Every example is compiled: each page is included into a `rustx` doc-test module through `#[doc = include_str!(..)]` under `cfg(doctest)`, so `rust` blocks run as doc tests (expansions that are not standalone use `ignore` only when the same code is already a `pass/` test, named next to the block). README links to the guide. Also fixes the stale "string literals into `String` fields are M8" comments in `tests/pass/into_params.rs` and `tests/cases/into_params.rustx` (re-blessed; only the comment line changes). The `rustx-cli fmt` page is added when T31 merges. Verify: `just check` runs every guide example; every sugar item in `passes/mod.rs` has a page.
