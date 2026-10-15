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
