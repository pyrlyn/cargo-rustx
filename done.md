# Done

### T1. M0: workspace skeleton

Cargo workspace with `rustx-core` (token passes → `syn::File` → AST passes), `rustx-macros` (`sugar!`) and the `rustx` facade. `sugar!` passes valid Rust through unchanged. insta snapshot tests over `tests/cases/*.rustx`, trybuild pass/fail tests, `just check`, GitHub Actions CI on Linux, macOS (arm64) and Windows plus an MSRV job.

### T2. M1: `var` statements

Problem 02. `var x = e;` and `var x: T = e;` (and `var` patterns where `let mut` is valid) desugar to `let mut`. First real token pass: establishes how a pass finds statement position and keeps spans.

Done: `var` at statement start becomes `let mut` with the span of `var`; identifier uses pass through. Destructuring, `var mut`, `var ref` and `var _` are rejected with targeted errors (`var` means exactly `let mut <name>`). Sugar also expands inside macro-call arguments (recorded in ARCHITECTURE.md limitations).

### T3. M2: type-prefix sugar `~Trait`, `iter T`, `rc T`, `weak T`

Problems 04, 09, 16 (types only; `rc e` expressions and field access belong to M6). `~T` → `Box<dyn T + Send + Sync>`, `iter T` → `impl Iterator<Item = T> + '_`, `rc T` → `Rc<RefCell<T>>`, `weak T` → `Weak<RefCell<T>>`, with fully qualified std paths so user imports do not matter.

Done: the four prefixes in type positions, end of the type found with syn's type parser, nested prefixes expanded right to left; `~` without a trait is an error. Not covered here: prefixes at the start of a group or after `,` outside `<..>` (tuple types, fn-pointer parameters), and `iter T` with `from(..)` lifetimes (M3).

### T4. M2: method generics without turbofish

Problem 06. `.m<T>(..)` → `.m::<T>(..)`; `.collect<Vec>()` and other known std collections get their missing arguments as `_` (`Vec<_>`, `HashMap<_, _>`).

Done: `.m<..>(` becomes `.m::<..>(` when the angle content parses as generic arguments and a call group follows; bare `Vec`, `VecDeque`, `LinkedList`, `BinaryHeap`, `HashSet`, `BTreeSet` get `<_>`, `HashMap`, `BTreeMap` get `<_, _>`. Known ambiguity `(a.b < c, d > (e))` is recorded in ARCHITECTURE.md limitations.

### T5. M3: inline bounds in parameters

Problem 03. `x: T: Clone + Debug` in fn parameters hoists the bound into the fn's generics / `where` clause; the generic parameter is declared once, existing generics and `where` clauses are kept.

Done: the token half marks `x: T: B` parameters (and `E: Bound` inside return type paths) with a hidden attribute; the AST half declares new generics and adds `where` predicates, merging repeats. A name in scope only gets the bound; a type declared or imported in the input is an error. Structs, enums and unions declared in the input lend their bounds to `impl` headers and signatures that use them with undeclared parameters (problem 03's `impl Cache<K, V>`). Limits: glob-imported types are unknown; signatures inside macro bodies are not lowered; closures and `&T: Bound` are not supported.

### T6. M3: `bound` presets

Problem 21. `bound Key = Clone + Eq + Hash;` at item level becomes a trait with those supertraits plus a blanket impl, usable everywhere a bound is.

Done: `[vis] bound Name = A + B;` at item position becomes a trait plus a blanket impl over a reserved `__RustxSelf` parameter; `Self` in the bounds maps to it; `#[cfg]` is copied to the impl. Generic presets, a missing `=` or `;` and an empty list are errors on the user's tokens.

### T7. M3: `&from(x) T` return lifetimes

Problems 01 and 09 (`from` combined with `iter T`). `-> &from(xs) Item` introduces a lifetime, adds it to the fn generics and to the reference parameter `xs`; `from(a, b)` ties several parameters; `iter T` with `from(..)` uses that lifetime instead of `'_`.

Done: `&from(a, b) T` becomes a marker lifetime, then the AST pass gives the named parameters one shared `'__rustx_from_*` lifetime (references, `self` receivers, `impl Trait` and bare type parameters with reference bounds) and replaces `'_` of a return `impl Trait` that contains it, so `iter &from(x) T` works. Unknown names, non-reference parameters and `from` outside a return type are errors. Limits: `from(Self)` belongs to T8; destructured parameters cannot be named; only the outer reference of `&Vec<&str>` is tied.

### T8. M3: `#[borrows]` structs and `&from(Self)`

Problem 19. `#[borrows] struct Lexer { src: &str }` adds one lifetime to the type, its fields and its `impl` headers; `&from(Self) T` / `&from(Type) T` resolve to that lifetime. Depends on T7.

Done: `#[borrows]` on a struct or enum adds `'__rustx_borrows` first in its generics and puts it on every elided `&`/`'_` in fields and on nested `#[borrows]` types; impls naming such a type get it in their header and signatures (elided `&` in signatures stays elided). `from(Self)` uses the impl's lifetime; `from(Type)` in other fns adds the lifetime and ties every `Type` parameter. Errors: arguments to `#[borrows]`, a type with nothing to fill or with its own lifetime, `from(Self)` outside a borrowing impl, `from(Type)` with no such parameter, `from(Self, x)`. Limits: types match by last path segment; fn bodies are not rewritten; `fn(&T)` keeps its own elision.

### T9. M3: `into T` parameters

Problem 15. `name: into String` becomes `name: impl Into<String>` plus `let name: String = name.into();` at the top of the body.

Done: the token half marks `x: into T` parameters; the AST half gives them `impl ::core::convert::Into<T>` and prepends a fully qualified conversion in parameter order, moving `mut`/`ref` to the `let` and copying `cfg`/lint attributes. Declarations without a body (trait items, `extern` fns) only get the type. `into` on `self`, on a pattern or without a type is an error.

### T10.1. M3: parameter defaults in signatures

Problem 17 (parameters only; field defaults belong to M4). `port: u16 = 8080` becomes `port: Option<u16>` with `let port = port.unwrap_or(8080);` (or `unwrap_or_else` when the default is not a literal) at the top of the body.

Done: the token half turns `x: T = e` into a `#[__rustx_default(e)]` marker (the contract T10.2 reads through `param_defaults::default_of`); the AST half makes the type `Option<T>` and prepends `unwrap_or` for constant defaults or `unwrap_or_else(|| ..)` otherwise, so a default can use earlier parameters. Patterns move into the `let` behind a generated name. Defaults must be trailing; a default on `self`, an empty default and a default on a fn without a body are errors. Limits: a bare `<` in a default needs parentheses; `?`/`return`/`.await` in a lazy default act on the closure.

### T10.2. M3: default arguments at call sites

Problem 17. Calls inside the same `sugar!` input to a fn with parameter defaults: omitted trailing arguments become `None`, given ones at defaulted positions `Some(..)`. Callers elsewhere see the real `Option<T>` signature. Split from the original T10.2 to fit the task size; depends on T10.1.

Done: AST pass `passes/call_defaults.rs` (before `param_defaults::lower`) records every signature, then rewrites calls: given arguments at defaulted positions become `Some(..)`, omitted ones `None`. Path calls resolve by last segment (and qualifying type or module, `Self` included), `.m(..)` calls by method name; same-named fns that would read a call differently are an ambiguity error. Calls inside parameter defaults and inside macro bodies that parse as `a, b` or `x; n` are rewritten too. Too many arguments and a missing required argument are errors.

### T10.3. M3: named arguments at call sites

Problem 17. `connect("db", port: 6432)` inside the same input names a defaulted parameter; arguments are reordered to the declaration and the rest filled as in T10.2. Named arguments are allowed only for defaulted parameters. Depends on T10.2.

Done: the token half turns `name: value` at the start of a call argument into a reserved `__rustx_named!(name = value)` marker (never in fn parameter lists, fn-pointer types, macro groups, struct literals or closure parameters); the AST half places it by parameter name. Errors: unknown name, a name for a parameter without a default, a duplicate, a positional argument after a named one, a named argument to a call with no defaulted fn. Markers in attribute arguments or opaque macro bodies are restored to `name: value`. Limits: named arguments are evaluated in declaration order; calls with generic arguments take none.

### T11. Shared token helpers for passes

The mandatory no-duplication rule: after T2–T7 several passes carry private copies of the same token logic — punct checks (`is_punct`, joint/alone), statement/item-start detection (`var`, `bound_presets`), outer-attribute and visibility skipping, angle-bracket depth and matching `>` (with `->` handling) in `turbofish`, `prefix_types` and `inline_bounds`, and error accumulation (`syn::Error::combine` helpers). Move one copy of each into `tokens.rs` (or a `tokens/` module), switch every pass to it, change no behaviour. Blocks T8–T10 so they start on the shared helpers.

Done: shared helpers live in `tokens.rs` and `tokens/{punct,position,angle,syntax,params}.rs` (punct predicates, `->` detection, statement/item start, angle depth and matching `>`, error combining, bare names, the error-collecting walker, and the fn-parameter walker for parameter passes). No snapshot or compile-test output changed. Deliberately separate: `var`'s attribute rule, `prefix_types`' backward generics scan and single-user helpers.

### T12. M4: `derive` presets

Problem 18. `derive Id = Debug, Clone, Copy;` at item position declares a preset; in every `#[derive(..)]` of the same input a preset name expands to its list, deduplicated in order, so presets mix with ordinary derives. A preset that shadows a std derive name is an error.

Done: pass `passes/derive_presets.rs` (with `lower` and `marker` submodules). The token half removes `derive Name = ..;` items and records each as a file-level `#![__rustx_derive_preset(Name = A, B)]` marker; the AST half expands preset names in every `#[derive(..)]` of the input, deduplicated in order. Errors: bad syntax, an empty list, a preset shadowing a std derive, a preset naming itself. `visibility_start` moved to `tokens/position.rs` and is shared with `bound_presets`.

### T13. M4: field defaults and `S { .., .. }`

Problem 17 (fields). `pub port: u16 = 8080` in a struct generates `impl Default`, and a struct literal ending in a bare `..` becomes `..Default::default()`. Follows RFC 3681 so the pass can switch to the native form once it is stable.

Done: AST-only pass `passes/field_defaults.rs`, since `syn` parses RFC 3681 itself. It generates `impl Default` after the struct (generics, `where` and `cfg` copied; a string literal on a `String` field goes through `Into::into`) and fills a bare `..` in struct expressions, never on the left of an assignment and in macro bodies only inside std expression macros. Errors: `derive(Default)` next to field defaults, defaults on enum variant or union fields. Macro-body expression parsing is shared with `call_defaults` in `passes/macro_args.rs`.

### T14. M4: `#[data]` constructors and getters

Problem 08. `#[data] struct User { name: String, age: u32 }` generates `new` (all fields), getters returning `&T` (by value for `Copy` primitives) and an optional builder, as the problem file specifies.

Done: AST pass `passes/data.rs` with `data/{methods,builder}.rs`. `#[data]` on a named-field struct at any depth adds `new`, getters (by value for primitives and shared references, `&str` for `String`, `&T` otherwise) and, with flags, `set_*`, `with_*` and a `NameBuilder` whose `build` reports a missing field. Defaults come from `#[default(expr)]` or `field: T = expr` (the pass runs before field defaults and shares their value conversion). `#[borrows]` structs name their lifetime in parameters and get a `#[borrows]` builder. Errors: non-named-field items, unknown, repeated or valued flags, `#[default]` without a value or given twice or next to `= value`, a builder that would collide with a field or a declared type.

### T15. M4: `from` variants and `#[display]`

Problem 05. `enum MyErr { Io(from io::Error) }` generates `From<io::Error> for MyErr`; `#[display(..)]` on an enum or its variants generates `Display` (and `Error` where the problem file asks for it), like thiserror / derive_more.

Done: pass `passes/from_display.rs` with `from_display/{format,lower}.rs`. `Io(from io::Error)` generates `From`, `#[display("..")]` on every variant generates `Display` and `Error`, with positional and named placeholders checked against the variant's fields. Generic enums keep their generics without added bounds. Errors: `from` on a multi-field variant, a type converted twice, a variant missing `#[display]`, a placeholder naming no field, width or precision arguments.

### T16. M4: `#[delegate]` fields

Problem 07. `#[delegate(Read)] inner: File` generates `impl Read for Wrapper` forwarding every method to the field.

Done in two commits to fit the task size (fields, then `except`). AST pass `passes/delegate.rs` with `delegate/{spec,std_traits,forward,except}.rs`: `#[delegate(Trait, ..)]` on a field of a named or tuple struct generates `impl Trait` forwarding the trait's required items through the trait path. Methods come from an explicit list `Trait { fn m(&self); }`, a trait declared in the input, or a std table (`Read`, `Write`, `BufRead`, `Seek`, `Iterator`, `DoubleEndedIterator`, `Display`, `Debug`, `Hash`). `except(m, ..)` merges the rest into a hand-written impl. `#[borrows]` structs get their lifetime in associated types. Errors: unknown trait, enum or union fields, a trait delegated twice, bad syntax, `except` problems, fns without `self`, typed receivers, generic associated types, ambiguous local traits.

### T17. M4: `#[newtype]`

Problem 20. `#[newtype(deref, display, from_str, validate = check)] pub struct Email(String);` generates one impl per flag, plus `new` (fallible with `validate`).

Done: AST pass `passes/newtype.rs` with `newtype/impls.rs`. `#[newtype(..)]` on a one-field tuple struct at any depth adds `new` and one impl per flag in the order written: `deref` and `as_ref` (target `str` for `String`, `[T]` for `Vec<T>`, else the inner type; never `DerefMut`), `display` (forwarding, or `display = ".."` with `{}`/`{0}` as the inner value), `from_str`, `into`, and `add`/`sub`/`mul`/`div`/`rem`. `validate = f` makes `new` return `Result<Self, E>` with `E` read from `f`'s signature next to the struct, and `from_str` goes through `new`. Errors: not a one-field tuple struct, repeated attribute or flag, unknown flags, stray values, a missing or ill-typed validator, operators together with `validate`. Shared afterwards: `passes/impl_header.rs`, `passes/attr_flags.rs`, `tokens::syntax::{std_type, is_string}`.

### T18. M4: standalone attribute macros

Roadmap M4: `#[data]`, `#[display]`, `#[delegate]`, `#[newtype]` and `#[sugar]` usable as attribute macros on valid Rust items outside `sugar!`, reusing the AST passes of T13–T17 through `rustx-core`. Depends on T14–T17.

Done in two commits (the macros, then `#[newtype]`). `rustx_core::expand_attribute(AttributeMacro, args, item)` rebuilds the item into a `syn::File` and runs the pass behind each attribute: `#[data]`, `#[display]` (with `#[from]` on a variant's field), `#[delegate]`, `#[newtype]`, and `#[sugared]` for every AST pass on any item. The whole-item attribute is `#[sugared]` because a crate cannot export a bang macro and an attribute macro under one name. Thin `#[proc_macro_attribute]` shells live in `rustx-macros` and are re-exported from `rustx`. Errors are emitted next to the kept item; attributes that would generate nothing are errors; pass errors that point next to the item are reworded for the standalone form, matched on the passes' own message constants. Limits: a standalone attribute sees only its item (no local traits or `except` for `#[delegate]`, no `validate` function for `#[newtype]`); field defaults and `derive` presets need `sugar!`.

### T23. M6: `Shared<T>` runtime type

Problem 12. A `Shared<T>` type in the `rustx` facade wrapping `Arc<Mutex<T>>`, with `Shared::new`, `Clone`, a lock accessor the `shared` sugar desugars to, and `with(|v| ..)` / `with_mut(|v| ..)` for an atomic group of operations. The `rustx` crate's first runtime code. Done when its API has doc tests and unit tests, including poisoning behaviour.

Done: `crates/rustx/src/shared.rs`, re-exported as `rustx::Shared`. `Shared<T>` wraps a private `Arc<Mutex<T>>` with `new`, `lock` (the guard the `shared` sugar desugars field access to), `with`, `with_mut`, `is_poisoned`, a hand-written `Clone` (no `T: Clone`), `Default`, `From<T>` and a `Debug` that uses `try_lock`, so it does not deadlock. A poisoned lock is recovered with `PoisonError::into_inner` instead of `.lock().unwrap()`: the panic already reaches its owner, the sugar makes a lock look like field access with no failure path, and the workspace lints forbid `unwrap`; `is_poisoned` exposes it. Left for T24: a guard used as a temporary lives to the end of its statement, so two accesses to one binding in one statement must share a guard or go through `with`.

### T30. M9: `rustx-cli expand`

A `rustx-cli` binary whose `expand <file>` prints the plain Rust a `sugar!` body or `.rustx` file desugars to, formatted with `prettyplease`, and reports errors with file positions. T19 builds its reverse command on the same CLI. Done when integration tests run the binary on the problem examples and compare with snapshots.

Done: new crate `crates/rustx-cli` (binary `rustx-cli`, `clap` derive). `expand <file>` desugars a `.rustx` file as a whole, or replaces every `sugar!`, `rustx::sugar!` and `::rustx::sugar!` call at any depth of a `.rs` file (including calls an expansion produces), and prints the result with `prettyplease`. Errors are printed as `path:line:column: message` (1-based, from `proc-macro2` `span-locations`, enabled for the CLI only) with exit code 1; I/O and closed-pipe errors do not panic. Limits: non-doc comments are dropped, `sugar!` inside other macros' arguments and attribute macros are not expanded, a `sugar!` in expression position is an error. T19 and T31 add a `Command` variant and reuse `diagnostic::render`; T32 should move `expand::expand_source` and `diagnostic::render` into a library rather than copy them.

### T22. M6: `move[..]` capture lists

Problem 11. `move[a.clone(), b.clone()] || work(a, b)` becomes `{ let a = a.clone(); let b = b.clone(); move || work(a, b) }`, keeping the user's names. The form stays close to `move(expr)` (RFC 3968) so the desugaring can switch to native syntax later without changing user code. Done when unit tests, a snapshot case and `pass/` and `fail/` tests cover plain and `async` closures and captures that are not a place expression.

Done: token pass `passes/capture_lists.rs`, last in `TOKEN_PASSES` because it reads the closure after the list with `syn::ExprClosure`, which needs every other sugar inside it gone, and so ends the closure exactly where rustc does. `move[a.clone(), b] |..| body` and `async move[..] |..| body` become a block of `let` bindings followed by the `move` closure; sequences are processed from the right, so nested lists work. A capture binds the variable it starts with, followed only through method calls (`a.clone()`, `a.lock().unwrap().clone()`, `a`). Errors: empty list, a capture with no name to take (field access, `&a`, paths, `self`), a name captured twice, and a list not followed by a closure (`move[..] async ||` and async blocks included). Limit: like `var`, it also rewrites inside macro-call arguments.

### T20. M5: `T throws` return types and tail `Ok(..)`

Problem 10. `fn load() -> Config throws` returns `anyhow::Result<Config>` and its tail expression is wrapped in `Ok(..)`. `anyhow` is reached through a re-export in the `rustx` facade (the `throws` result alias of ARCHITECTURE b), so users do not add it themselves. Done when the problem's detailed example expands to its "As is" code apart from the error type, with unit tests, a snapshot case, `pass/` and `fail/` tests; which tails are left alone (one that is already `Ok(..)`/`Err(..)`, `?`-only bodies, a body ending in `return`) is settled with the creator when the task is claimed.

Decided with the creator: `T throws` returns `anyhow::Result<T>` (through the `rustx` re-export) and `T throws E` returns `Result<T, E>`. The tail is wrapped in `Ok(..)` unless it already is `Ok(..)` or `Err(..)`, or does not produce a value (`return`, `panic!`, `unreachable!`, `todo!`, `unimplemented!`, `loop`); a body without a tail gets `Ok(())`.

Done: token pass `passes/throws.rs::rewrite` (right after `prefix_types`, so `iter &T` is rewritten before it sits after `<`) and AST pass `throws::lower` (last). `-> T throws` becomes `-> ::rustx::anyhow::Result<T>` and `-> T throws E` becomes `-> ::core::result::Result<T, E>`; `rustx` re-exports `anyhow` 1.0.104. The token pass leaves `#![__rustx_throws]` as the first inner attribute of the body; the AST pass wraps the tail in `Ok(..)` unless it is already an `Ok`/`Err` call or diverges (`return`, `panic!`-like macros, a `loop` no `break` leaves), appends `Ok(())` to a body without a value tail (or with a `for`, `while` or `if` without `else` tail) unless its last statement diverges, and removes the marker. The tail is wrapped as a whole; `if`/`match` arms are not entered. `throws` is sugar only in a fn return type after a token that can end a type, so `throws` as an identifier passes through. Errors: `throws` without `->`, `throws` twice, `throws` not followed by a type. `signatures::for_each_fn` now hands the attributes to the callback; `for_each_signature` wraps it. Limits: signatures inside macro bodies keep the marker, and a `break` inside a macro call is not seen.
