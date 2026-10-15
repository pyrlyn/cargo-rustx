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

### T11. Shared token helpers for passes

The mandatory no-duplication rule: after T2–T7 several passes carry private copies of the same token logic — punct checks (`is_punct`, joint/alone), statement/item-start detection (`var`, `bound_presets`), outer-attribute and visibility skipping, angle-bracket depth and matching `>` (with `->` handling) in `turbofish`, `prefix_types` and `inline_bounds`, and error accumulation (`syn::Error::combine` helpers). Move one copy of each into `tokens.rs` (or a `tokens/` module), switch every pass to it, change no behaviour. Blocks T8–T10 so they start on the shared helpers.

Done: shared helpers live in `tokens.rs` and `tokens/{punct,position,angle,syntax,params}.rs` (punct predicates, `->` detection, statement/item start, angle depth and matching `>`, error combining, bare names, the error-collecting walker, and the fn-parameter walker for parameter passes). No snapshot or compile-test output changed. Deliberately separate: `var`'s attribute rule, `prefix_types`' backward generics scan and single-user helpers.
