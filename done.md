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

### T25. M6: `rc e` expressions

Problem 16. `rc e` in expression position becomes `Rc::new(RefCell::new(e))` (the `rc T` / `weak T` types landed in M2). Expression position is told apart from type position as ARCHITECTURE h describes. Done when unit tests, a snapshot case and `pass/` tests cover arguments, `let` values and struct field values, and valid Rust using `rc` as an identifier still passes through unchanged.

Done in two commits (the `prefix_types` change, then the pass). Token pass `passes/rc_values.rs`, right after `prefix_types`: `rc e` becomes `::std::rc::Rc::new(::std::cell::RefCell::new(e))`. `rc` is sugar only as a plain identifier (not `r#rc`, not after `.`, `::`, `$`, a lifetime quote or `#`) followed by a token that cannot follow a name in valid Rust and starts a value: an identifier other than `as`, `else`, `for`, `if`, `in`, `where`, a literal, or a `!` not glued to `=` and not followed by a group. The value runs like the operand of `return`, to the next `,` or `;` or the end of its group; commas inside turbofish generics and closure parameters do not end it; nested `rc` expand inside out. Error: `rc` followed by a keyword that starts no expression (usually a type in a tuple-type position). `prefix_types` now reads `rc` after a single `:` as a type only when the type ends at `,`, `;`, `=`, `|` or the group end and is not a macro, so struct literal field values reach the value sugar. Limits (ARCHITECTURE h): a value starting with `&`, `-`, `*`, `|`, `<` or `if` needs the plain spelling, and `S { f: rc x }` with a bare name still reads as a type, so it is written `S { f: (rc x) }`.

### T19.1. Sugar conversion driver and `rustx-cli sugar`

The shared part of T19: a reverse-rule driver in `rustx-core` that finds code a pass would have generated and rewrites only those source ranges, so comments and layout elsewhere survive; a `rustx-cli sugar <file>` command that prints the converted source; and the first rule, `let mut` back to `var`. Done when the command works on a file, a property test helper checks `expand(sugar(x))` against `x` after normalisation, and valid Rust with no matching shape comes back byte-for-byte unchanged.

Done: `rustx_core::sugar(source) -> syn::Result<String>` behind the core feature `sugar` (`proc-macro2/span-locations`, `syn/visit`), enabled only by `rustx-cli`, so the proc-macro build stays without it. Rules implement `ReverseRule { fn edits(&self, file, source) -> Vec<Edit> }` with byte ranges from spans (`Edit::between`) and are listed in `sugar::RULES`. The driver refuses a file that expansion already changes as written, applies non-overlapping edits to the original text (the earlier rule wins), and keeps an edit set only if expanding the result is token-equal to the original, falling back to adding edits one at a time, so comments and layout survive. First rule `sugar/var.rs`: `let mut x [: T] [= ..];` with a mutable by-value name becomes `var x ..`; destructuring, `ref mut`, `if let`, match arms, closure parameters, macro arguments and a comment between `let` and `mut` are left alone. `rustx-cli sugar <file>` prints the whole file as `.rustx` text. Test helper `sugar::tests::assert_sugar(plain, expected)` checks the output and that it expands back. Open for T19.2 onward: passes emit fully qualified paths (`::std::rc::Rc::new`, `::core::convert::Into`), so a rule matches only code spelled that way unless the check learns to equate known std paths with their short forms.

### T21. M5: `throw e`

Problem 10. `throw e` inside a `throws` fn becomes `return Err(::core::convert::Into::into(e))`; outside one it is an error at `throw`. Depends on T20. Done when unit tests, a snapshot case and `pass/` and `fail/` tests cover it, including `throw` inside closures and nested fns, which belong to their own signature.

Done: token pass `passes/throw.rs` right after `throws::rewrite`. In a body marked by `throws` (the marker now records the form: `#![__rustx_throws(anyhow)]` for plain `throws`), `throw e` becomes `return ::core::result::Result::Err(::rustx::anyhow::anyhow!(e))`, or `Err(::core::convert::Into::into(e))` under `throws E`, with the span of `throw`. `throw` is sugar at a statement start (the shared `tokens::position::starts_statement`, moved out of `var`) or after `=>`, `=`, `|`, when an identifier other than `as`, `if`, `else` or a literal follows; the value runs to the next `;` or `,` or the end of its group. The pass walks top-down with its own recursion to carry the enclosing error form; unmarked fns, closures (including `move[..]`), `async` and `const` blocks clear it, and a nested `throws` fn uses its own. Error: `throw` outside a `throws` fn. A string literal goes into `anyhow!` as a format string. Limits: `throw (e)`, `throw &e`, `throw ::path` stay identifier uses; a match arm with a leading `|` is read as a closure; like `var`, macro inputs are rewritten. The `throws` case and pass test now use `throw`.

### T27. M7: `async fn` in traits for `dyn` use

Problem 14. A trait with `async fn` stays native when it is not used as `dyn`; for `dyn` use the macro generates code for the maintained `dynosaur` crate instead of hand-rolled boxing (research.md). How a trait is marked for `dyn` use is settled with the creator when the task is claimed. Done when the problem's detailed example compiles, including a `dyn` call, with unit tests, a snapshot case and `pass/` and `fail/` tests; split into subtasks at claim time if it exceeds the task size.

Decided with the creator: a trait is marked for `dyn` use by an attribute on the trait; without it the trait stays native.

Done in two commits (the pass, then the feature gate). Token pass `passes/dyn_traits.rs::rewrite` turns `#[dyn]` / `#[dyn(Name)]` on a `trait` (after other attributes, visibility, `unsafe`, `auto`) into a marker, since `syn` cannot parse a keyword attribute; `dyn_traits::lower` replaces it with `#[::rustx::dynosaur::dynosaur(<vis> <Name> = dyn(box) <Trait>)]`, default name `Dyn` + trait name with the trait's visibility. Unmarked traits stay native. `dynosaur` 0.3.1 brings `syn` 2, so by the creator's decision it is optional behind the `dyn` feature of `rustx` (forwarded to `rustx-macros`); `rustx_core::expand_with(input, Features { dyn_traits })` reports `#[dyn]` without the feature at the `dyn` token, naming the feature, while `expand`/`try_expand` allow everything. Errors: `#[dyn]` not on a trait, a malformed argument, `#[dyn]` twice. Limits (ARCHITECTURE h): `#[dyn]` works only inside `sugar!`, `dyn Trait` is not rewritten to `DynName` (values go through `DynName::from_ref` / `new_box`), dynosaur's `bridge(..)` is not exposed.

### T19.2. Reverse rule for the turbofish sugar

The first rule on the T19.1 driver after `var`: `.m::<T>(..)` back to `.m<T>(..)` exactly where the turbofish pass would produce it, and nothing that the forward pass would read differently (comparisons in comma lists, see ARCHITECTURE h). Done when the rule is in `sugar::RULES` with `assert_sugar` unit tests, a snapshot case, and valid Rust with no matching shape unchanged.

Done: `sugar/turbofish.rs`, second in `sugar::RULES` (its edits cannot overlap `var`'s). A method call with non-empty explicit generic arguments, `.m::<..>(..)`, loses only the `::` bytes, so comments in the arguments survive. Before emitting an edit the rule runs the forward token passes on `receiver.m<args>()` and keeps it only if they read it back to the same call, so it follows the pass without copying its tables. Left alone: path turbofish (`Vec::<u8>::new()`, `g::<u8>()`), an empty `::<>`, a bare std collection the pass would fill (`collect::<Vec>()`), and calls inside macro arguments. Ideas for later rules: a shared `reads_back(sugar, plain)` helper on the driver, and shortening `collect::<Vec<_>>()` to `collect<Vec>()` as problem 06 shows.

### T24. M6: `shared` bindings and `shared T` types

Problem 12. `shared s = e;` becomes a `Shared<T>` binding, `shared T` in type position becomes `Shared<T>`, and field access on such bindings goes through the lock, one lock per access, as the problem file shows; read-only data becomes a plain `Arc`. Depends on T23. The access rewriting is written so T26 can reuse it. Done when the problem's detailed example compiles and runs through `sugar!`, with unit tests, a snapshot case and `pass/` and `fail/` tests.

Done in two commits to fit the task size (the type and token pass, then accesses). `shared s = e;` at a statement start (the shared `starts_statement`) becomes `let s = ::rustx::Shared::new(e);` and `shared T` is a `prefix_types` prefix for `::rustx::Shared<T>`. AST pass `shared::lower` with the shared `passes/access.rs` (`Opening`, `scan`, `rewrite`, `MUTATING_METHODS`) rewrites field access and value-method calls on the binding through the lock; handle methods (`lock`, `with`, `with_mut`, `is_poisoned`, `clone`) stay. A unit (statement, block tail, `if`/`while` condition without `let`, `move` closure body, `async` block) with one direct access uses `s.lock().f`; a unit with more opens one guard, `{ let __rustx_s = &mut *s.lock(); .. }`, so two accesses in one statement cannot deadlock (a block, not `with_mut`, keeps `return`, `?`, `break` and `.await` working). A binding with accesses, no writes and no handle use becomes a plain `::std::sync::Arc`. Scope ends at a rebinding pattern, except `let s = s.clone();` (the `move[..]` form). Errors: `shared mut`, a pattern instead of a name, no value, a declaration inside a macro call. Limit: parameters and fields typed `shared T` are not tracked; users call `lock()` or `with` there.

### T28. M8: implicit lossless `into()`

Problem 13. Where the target type is written in the input (a `let` annotation, a field of a struct declared in the input, a parameter of a fn declared in the input), a lossless widening (`u32` to `u64`, `u16` to `u32`, ...) gets `.into()`; lossy conversions stay explicit `as`. Explicit opt-in, as ARCHITECTURE g says; the opt-in form is settled with the creator when the task is claimed. Done when unit tests, a snapshot case and `pass/` tests cover each position, and nothing changes without the opt-in.

Decided with the creator: opt-in through a `#[widen]` attribute on a fn, impl or module, or `#![widen]` at the start of the `sugar!` input; it applies only inside what is marked.

Done in two commits to fit the task size (typed positions, then the pass). `passes/typed_positions.rs` (`Declared::collect`, `walk_fn`, the `Convert` trait) finds where a written target type meets an expression, reusing `call_defaults::collect` for calls. AST pass `passes/widen.rs`, before `throws::lower`: inside a fn, impl, module or trait default method marked `#[widen]`, or under `#![widen]`, an expression whose source type is written (a `let` annotation, a parameter, `self`, a typed closure parameter, a `for` element over a slice, array or `Vec` of known type, a field of a declared struct, `*r`) becomes `<T as ::core::convert::From<S>>::from(e)` when std has that lossless `From` (unsigned, signed and float widenings; nothing from `usize`/`isize`). Positions: `let x: T = e`, assignment and compound assignment (not shifts) into typed locals and fields of declared structs, arguments of path calls to fns declared in the input. Shadowing is tracked per scope. Errors: `#[widen]` with arguments, twice, or on another item. Works under `#[sugared]` when written after it. Not converted: struct-literal fields, method-call arguments, `const`s, return positions.

### T32. M9: file-level `.rustx` via `build.rs`

A build helper that desugars `.rustx` files into `OUT_DIR` from `build.rs`, so a whole file can use the sugar and be `include!`d, with `rerun-if-changed` lines and errors that name the `.rustx` file and line. Done when an example crate in the workspace builds from a `.rustx` file and a test shows a sugar error pointing at the right `.rustx` line.

Done: new crate `crates/rustx-build`, which `rustx-cli` now also uses for expansion and diagnostics (it exports `Kind`, `expand_source` and `render`). `expand_file(path) -> Result<PathBuf, Error>` and `expand_dir(dir)` write `$OUT_DIR/<relative path>.rs` with a header comment and print `cargo::rerun-if-changed`. `Error::emit()` prints `cargo::error=` lines. Paths with `..`, absolute paths, non-UTF-8 paths and paths with line breaks are rejected. `span-locations` is enabled only on the host when `rustx-build` is a build dependency. The example `examples/build-rs` includes the expansion with `include!`. Limit: `#[data]` accessors carry no docs, so a crate with `#![deny(missing_docs)]` fails on them.

### T26. M6: borrows on `rc T` field access

Problem 16. On a binding whose type is written `rc T` or `&rc T`, field access gets `.borrow_mut()` when the field is assigned, compound-assigned or receives a known `&mut self` std method call, and `.borrow()` otherwise, plus `with` / `with_mut` helpers in `rustx`. Reuses T24's access rewriting. Depends on T24 and T25. Done when the problem's tree example runs through `sugar!`, with unit tests, a snapshot case and `pass/` and `fail/` tests.

Done: AST pass `passes/rc_borrows.rs` tracks `let` bindings and fn, method and closure parameters whose type is written `rc T` or `&rc T`, plus `let`/`var x = rc e`. Field accesses on them go through the shared `access::rewrite`: writes (assignment, compound assignment, `&mut`, `MUTATING_METHODS`) use `.borrow_mut()`, everything else uses `.borrow()`. A statement with several accesses opens one guard, so a read and a write in one statement do not panic. Handle methods (`borrow`, `borrow_mut`, `try_borrow`, `try_borrow_mut`, `with`, `with_mut`, `clone`) and bare uses stay as written. `prefix_types` and `rc_values` emit a `__rustx_rc` marker path that this pass removes, so a hand-written `Rc<RefCell<T>>` is never touched. Runtime: `rustx::RcExt` with `with` and `with_mut` (users import it; generated code never calls it). Not tracked: fields, untyped bindings, bindings declared in macro calls. The pass reports no errors.
