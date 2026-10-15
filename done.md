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
