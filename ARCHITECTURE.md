# Architecture (plan)

Status: M0–M4 are implemented; M5–M9 are planned.

## a) Approach: proc-macro crate vs standalone transpiler

| | Proc-macro crate (`sugar!` / `#[sugared]`) | Standalone transpiler (`.rustx` → `.rs`, build.rs or CLI) |
|---|---|---|
| Integration | `cargo add`, works in any crate, no build step | Needs build.rs / CLI, generated files, extra tooling |
| Input | Must tokenize as Rust tokens (`TokenStream`) | Arbitrary text, own lexer, any syntax |
| Error spans | Map back to source via `Span`; good on stable for most cases | Needs own source maps; rustc points at generated `.rs` unless remapped |
| IDE | rust-analyzer expands macros, but completion inside a custom-syntax body is weak | No IDE support for `.rustx` without a custom LSP |
| Type info | None | None (unless it calls rustc/rust-analyzer, which is out of scope) |
| Compile time | Macro runs on every build of the crate | Runs once per change, output is cached |

**Recommendation:** start with a proc-macro crate, function-like `sugar! { ... }` as the main path. Every sugar item in the vocabulary is expressible in Rust tokens, which removes the need for a custom lexer and gives span mapping for free. Keep the desugarer as a plain library (`rustx-core`) that works on `proc_macro2::TokenStream`, so a CLI transpiler (`rustx expand file.rustx`) can be added later on top of the same core without rewriting anything.

Crate layout:

- `rustx-core` — parser, AST, desugarer, diagnostics. Plain library, testable without proc-macro context.
- `rustx-macros` — thin `proc-macro = true` crate: `sugar!` and the attribute macros `#[sugared]`, `#[data]`, `#[display]`, `#[delegate]`, `#[newtype]`.
- `rustx` — facade crate re-exporting macros plus runtime helpers (`Shared<T>`, `with`/`with_mut` for `rc T`, the `anyhow` re-export behind `throws`).
- `rustx-cli` (later) — `rustx expand`, `rustx fmt`.

## b) Components

1. **Lexer.** None of our own: `proc_macro2::TokenStream` is the lexer. All sugar tokens (`var`, `~`, `throws`, `from`, `iter`, `shared`, `rc`, `weak`, `into`, `bound`, `derive`, `<` after a method name, `=` after a field or parameter type) are valid Rust tokens, just in invalid positions.
2. **Parser.** Two options:
   - Pure `syn` — only works when the input already parses as Rust. It does not (`var x = 1;` is not a statement).
   - **Custom parser built on `syn::parse::ParseStream`** (recommended): a fork of the relevant parts of the Rust grammar (items, fn signatures, types, statements, expressions) that accepts sugar forms and reuses `syn` for everything unchanged (`syn::Expr`, `syn::Type`, `syn::Path`, attributes). Sugar points are few and local, so the fork stays small: item start (`bound X = ..;`, `derive X = ..;`), statement start (`var`, `shared`), type position (`~T`, `&from(x) T`, `&from(Self) T`, `iter T`, `shared T`, `rc T`, `weak T`), fn return (`T throws`), fn params (`x: T: Bound`, `x: into T`, `x: T = default`), struct fields (`f: T = default`), struct literals (`S { a: 1, .. }`), call arguments (`f(a, name: v)`), method calls (`.m<T>(`), closures (`move[..] ||`), expressions (`throw e`, `rc e`), enum variant fields (`from T`).
   - **As built (M0):** a staged form of the custom parser. Sugar that `syn` cannot parse is rewritten by small token passes (`passes/*.rs`, each walking token sequences through one shared walker), which use `syn` parsers on token slices where they need grammar (e.g. where a type ends). The result is parsed as a `syn::File`, and sugar that needs context (lifetimes, `throws` tails) is rewritten by `VisitMut` AST passes. This keeps each sugar item an independent module, so items can be built in parallel; a full `ParseStream` grammar fork stays an option if token passes hit ambiguities they cannot resolve locally.
3. **AST.** `syn` types plus a small set of sugar node types (`SugarType::Boxed`, `SugarType::Borrow { from: Vec<Ident> }`, `SugarStmt::Var`, `SugarFn { throws: bool, inline_bounds }`, ...). Alternatively, parse sugar directly into `syn` nodes with marker attributes/placeholders and desugar in a second pass; this keeps the AST 100% `syn` and lets us use `syn::visit_mut`. Preferred: sugar AST only at the edges, lowered into `syn` immediately where the desugaring is local, and markers where it needs context (lifetimes, `throws` tail `Ok(..)`).
4. **Desugarer.** A set of independent passes, one per item, each a `VisitMut` over `syn::File`/`syn::ItemMod`:
   - `var` → `let mut`; `shared x = e` → `let x = Shared::new(e)`.
   - `~T` → `Box<dyn T + Send + Sync>`.
   - `&from(a) T` → introduce `'__a`, add to generics, rewrite param `a` to `&'__a`.
   - inline bounds → hoist into `Generics` / `where`.
   - `.m<T>(` → `.m::<T>(`, fill missing args with `_` for known std collections.
   - `T throws` → `::rustx::anyhow::Result<T>`, `T throws E` → `Result<T, E>`, wrap tail in `Ok(..)`, `throw e` → `return Err(e.into())`.
   - `move[a.clone(), ..] || body` → `{ let a = a.clone(); move || body }`.
   - `iter T` → `impl Iterator<Item = T> + '_` (or with the `from` lifetime).
   - `async fn` in traits → native AFIT, or `dynosaur`-generated boxing when the trait is used as `dyn`/`~` (maintained crate; no hand-rolled boxing).
   - `rc T` / `weak T` → `Rc<RefCell<T>>` / `Weak<RefCell<T>>`, `rc e` → `Rc::new(RefCell::new(e))`, `.borrow()`/`.borrow_mut()` on field access of bindings typed `rc T`.
   - `x: into T` → `x: impl Into<T>` plus `let x: T = x.into();`; string literals → `.to_string()` where the target is written as `String`.
   - field defaults → `impl Default` and `..Default::default()` for `S { .., .. }`; parameter defaults → `Option<T>` + `unwrap_or`, with calls in the same input filled in.
   - `#[borrows]` → one lifetime on the type, its fields and its `impl` headers; `&from(Self)` / `&from(Type)` → that lifetime.
   - `bound X = ..;` → trait with supertraits plus a blanket impl; `derive X = ..;` → expanded in every `#[derive(..)]` of the input.
   - `#[data]`, `#[delegate]`, `from` variants, `#[display]` → generated `impl` items (derive-like, can also be separate attribute macros).
   - implicit `into()` → only where the target type is syntactically known (see limitations).
   - `#[newtype(..)]` → one `impl` per listed flag.
   Pass order matters: `bound` and `derive` presets first; `#[borrows]` before `from` lifetimes; inline bounds and `from` lifetimes before `iter T`; `throws` after `throw`; derives last.
5. **Diagnostics / pretty-printer.** Every generated token carries the span of the sugar token it came from. Errors detected by the desugarer are emitted as `compile_error!` with that span; errors detected by rustc point at the original code because spans are preserved. `rustx-cli expand` uses `prettyplease` for readable output.

## c) Implementation language

Rust, with `syn` (full + visit-mut + extra-traits), `quote`, `proc-macro2`, `proc-macro-error2` or manual `compile_error!`, `prettyplease` for the CLI. Why:

- Proc macros must be Rust anyway.
- `syn` already parses 95% of what we need; we only write the sugar delta.
- `proc_macro2` makes the core testable outside the compiler (unit and snapshot tests).
- One language for users and contributors; no runtime dependency on Python/JS tooling.

## d) Entry points: `#[sugared]` vs `sugar!`

Key constraint: **an attribute macro on an item receives the item only after rustc has parsed it as valid Rust.** `#[sugared] mod m { fn f() { var x = 1; } }` fails in the parser before the macro runs. The same applies to `~Trait` in a type, `-> T throws`, `x: T: Clone`, `&from(a) T`, `move[..] ||`, `.parse<u32>()` (which parses as comparisons and fails or, worse, changes meaning).

Therefore:

- **`sugar! { ... }` (function-like) is the main path.** The body is an arbitrary token tree; it only needs balanced delimiters. Flow: `TokenStream` → custom `ParseStream` parser → sugar AST → lowering into `syn::File` → `VisitMut` passes → `quote!` → output.
- **`#[sugared]` (attribute) is the secondary path** for code that is already valid Rust and uses only attribute-shaped sugar: `#[data]`, `#[delegate]`, `#[display]`, `#[newtype]`, `#[borrows]` (references without a lifetime still parse; the error comes later, after the macro has added it), `from` on enum variants (spelled `#[from]`), and normalizations that act on valid syntax (e.g. `async fn` in traits → boxed variant, inline `Shared<T>`). Flow: `syn::parse::<Item>` → `VisitMut` → `quote!`. It is named `sugared` because `sugar!` holds the name: macros of every kind share one namespace, so a crate cannot export both.
- **As built (M4):** `#[data]`, `#[display]`, `#[delegate]` and `#[newtype]` run the one AST pass behind them on their item; `#[sugared]` runs every AST pass on any item, an inline module included. The item is rebuilt into a `syn::File` (with `#[data(..)]` or `#[newtype(..)]` put back, since their passes read them), token passes do not run, and errors are emitted next to the item, which is kept so code using it adds no cascade. `from` is spelled `#[from]` on the variant's field, `Io(#[from] io::Error)`, where `from` sits in the sugar; `#[display]` and `#[sugared]` read it, `sugar!` does not. `#[display]` needs `#[display("..")]` on its variants and `#[delegate]` a field with `#[delegate(..)]`, so an attribute that would generate nothing is an error. Field defaults (`f: T = v`) do not parse on stable and `derive` presets are token sugar, so neither is reachable here.
- A whole-file form: `sugar! { include!("foo.rustx") }` is not possible (include! is expanded later); a file-level form needs the CLI/build.rs transpiler — a later stage.

## e) Error handling and span mapping

- Keep spans: every rewritten node is built with `quote_spanned!(orig.span()=> ...)`, so `var x` → `let mut x` points at `var`.
- Hygiene: use `Span::mixed_site()` for helper locals (`__rustx_tmp`) and introduced lifetimes so they cannot collide with user names; use `Span::call_site()`/user spans for user-visible identifiers.
- `Span::resolved_at` / `located_at` (stable on `proc_macro2`) combine "resolve like the macro, report like the user token".
- Desugarer errors (e.g. `from(y)` names no parameter, `throws` on `main` without `Result`) → `syn::Error::new(span, msg).to_compile_error()`; collect several with `syn::Error::combine` instead of stopping at the first.
- `Span::join` (to underline a whole range) is nightly-only on `proc_macro`; on stable fall back to the first token's span. Document this.
- rustc type errors inside generated code: we cannot rewrite them; best effort is good spans plus naming generated items predictably (`__rustx_*`) so messages stay readable.

## f) Testing

- **Snapshot tests (`insta`)** in `rustx-core`: `tests/cases/*.rustx` → expanded Rust, formatted with `prettyplease`, stored as `.snap`. One case per sugar item plus combined cases (the detailed examples from `problems/` are the first fixtures).
- **Compile tests (`trybuild`)**: `tests/pass/*.rs` must compile; `tests/fail/*.rs` must fail with a given `.stderr` (bad `from(..)`, `throws` misuse, `move[..]` with non-ident). Checks that error spans point at the right place.
- **Roundtrip tests**: as-is code from each `problems/*.md` must compile (done manually in a scratch crate on Rust 1.97.1, edition 2024, for all 21 items), and the desugared output must be equivalent to it (compare `prettyplease` output or run behavioural unit tests).
- **`cargo expand`** for manual debugging; `rustx-cli expand` later.
- Property test (later): any valid Rust input to `sugar!` passes through unchanged (sugar is a strict superset).

## g) Roadmap

1. **M0 skeleton:** workspace, `rustx-core`, `rustx-macros`, `sugar!` that passes valid Rust through unchanged; insta + trybuild wired; CI.
2. **M1 statements:** `var` → `let mut` (item 2). Establishes the custom parser for statements and span plumbing.
3. **M2 types (purely syntactic):** `~Trait` (4), `iter T` (9), turbofish `.m<T>()` (6), `rc T`/`weak T` (16).
4. **M3 signatures:** inline bounds (3), `bound` presets (21), `&from(x) T` lifetimes (1), `#[borrows]` (19), `from` combined with `iter`, `into T` parameters (15), parameter defaults with call-site defaults and named arguments (17).
5. **M4 derives:** `#[data]` (8), `from` variants + `#[display]` (5), `#[delegate]` (7), `derive` presets (18), `#[newtype]` (20), field defaults (17). Also usable as standalone attribute macros.
6. **M5 errors:** `T throws` + `throw` + tail `Ok(..)` (10).
7. **M6 closures and sharing:** `move[..]` (11), `shared` + `Shared<T>` runtime type (12), `rc` field access (16).
8. **M7 async:** `async fn` in traits, boxed for dyn use (14).
9. **M8 conversions:** implicit `into()` in syntactically typed positions only (13), string literals into `String` (15); explicit opt-in.
10. **M9 tooling:** `rustx-cli expand`/`fmt`, file-level `.rustx` via build.rs, docs.

## h) Limitations

- **No type information.** Macros see tokens, not types. Implicit `into()` (13) can only fire where the target type is written in the source (`let x: u64 = ...`, struct literal of a struct declared in the same `sugar!`). Field `+=` needs the field type from the same macro input. Anything else stays explicit.
- **No type inference.** `collect<Vec>` → `Vec<_>` works for known names only; for user generic types the arity is unknown unless declared in the same input.
- **No cross-item info.** A macro sees only its own input. `#[delegate(Read)]` must know the trait's methods: works for a built-in list of std traits (`Read`, `Write`, `Iterator`, `Display`, ...) or traits declared in the same `sugar!`; otherwise the user lists methods explicitly.
- **`#[delegate]` forwards required items only** (`Read::read`, not `read_exact`), so provided methods go through the wrapper's own methods, hand-written overrides included. The list form is `#[delegate(Trait { fn m(&self); })]`; a trait of the input with generic parameters needs it too. `except(m)` adds the other items to a hand-written `impl Trait for Type` next to the struct, matched by last path segment.
- **`shared` field access** inserting `.lock().unwrap()` per access is not atomic across statements; needs a `with(|m| ..)` form and clear docs.
- **`rc` field access** picks `.borrow_mut()` from a built-in list of `&mut self` std methods; user methods need `with_mut(|n| ..)`. `RefCell` still panics at run time on conflicting borrows.
- **Parameter defaults and named arguments** only work for calls inside the same `sugar!` input. Callers elsewhere see the real `Option<T>` signature.
- **Call-site defaults match calls by name.** A path call resolves by its last segment (and the qualifying type, when it is one declared in the input), a method call by its name alone, so a defaulted method `len` also rewrites `vec.len()`. Same-named fns that would read a call differently make it an error; `<T as Trait>::f(..)` is not resolved. Named arguments are evaluated in declaration order, not in the order written, and a call with generic arguments (`f::<T>(..)`) takes none.
- **`#[widen]` (13) converts only between written number types.** It acts inside fn bodies of what it marks (a fn, `impl` or module, or `#![widen]` for the whole input), on pairs std converts with `From` (so `u16 -> usize` but never `usize`/`isize` to anything, `u16 -> isize`, `u64 -> f64` or `i32 -> f32`). The target must be a `let` annotation, a local or field (assigned or compound-assigned, shifts excepted) or a parameter of a fn of the input called by path (`f(x)`, `Type::f(x)`); struct literal fields and method call arguments stay explicit; the value must be a name bound with a written type (`let`, parameter, closure parameter, `for` over a slice, array or `Vec` of known type), a field of a struct of the input reached from one, or `*` of a known reference. Literals, arithmetic, method results and block values stay as written. Structs and fns are matched by name like call-site defaults, and a name declared twice with different types is left alone. The conversion is written `<T as From<S>>::from(e)`, so a binding the macro cannot see (one made inside a macro call) fails to compile instead of converting silently. `#[widen]` also works under `#[sugared]`, written after it.
- **`#[borrows]`** adds exactly one lifetime. Types with two independent lifetimes keep explicit parameters. Borrowing types are matched by their last path segment across the whole input, ignoring modules.
- **Field defaults and `S { .. }`** are read by `syn` (RFC 3681 syntax), so struct patterns keep their `..`. A bare `..` becomes `..Default::default()` in every struct expression, including structs declared outside the input, but inside macro arguments only for std expression macros (`vec!`, `assert_eq!`, `format!`, ...), never in `matches!` or user macros. Bounds are copied, not inferred: a generic field without a default needs `T: Default` on the struct. Enum variant and union fields take no defaults.
- **A bare `<` in a parameter default** reads as the start of generics, so `ok: bool = a < b` needs parentheses: `ok: bool = (a < b)`. A non-constant default runs in a closure, so `?`, `return` and `.await` inside it act on that closure.
- **`#[data]` judges getters by the written type.** Primitives and shared references are returned by value, `String` as `&str`, anything else as `&T`, so a user `Copy` type gets a `&T` getter. Field defaults for `new` are written `#[default(expr)]`; `field: T = e` (problem 17) is not read by `#[data]`.
- **`derive` and `bound` presets** are visible only inside the input that declares them (`bound` produces a real trait, so it can be exported; `derive` presets cannot).
- **Edition differences:** on edition 2024, `-> impl Trait` already captures all lifetimes, so `iter T` mainly saves typing there. The expansion keeps `+ '_` to work on both editions.
- **Upstream overlap:** default field values, trait aliases and capture expressions exist on nightly (see [research.md](research.md)). Each pass must be able to switch to the native form once it is stable, without changing user code.
- **IDE / rust-analyzer:** rust-analyzer expands proc macros, so go-to-definition and types work on the output, but completion and highlighting inside `sugar!{}` are limited, and rustfmt does not format macro bodies. A `rustx fmt` tool is needed.
- **Nightly-only span APIs:** `Span::join`, `Span::source_text` (partially), `Diagnostic` with notes/help are unstable; stable output uses `compile_error!` with single-token spans.
- **Sugar inside macro arguments is expanded too.** Token passes also rewrite the argument groups of macro calls (`vec![..]`, `assert_eq!(..)`), because users expect sugar there. A user macro that itself accepts tokens like `var x` receives the desugared form instead.
- **`.m<T>(` inside a comma list.** `(a.b < c, d > (e))` is valid Rust (two comparisons) but reads as the turbofish sugar `a.b::<c, d>(e)`. Parenthesise the comparison: `((a.b < c), d > (e))`.
- **Type prefixes need a recognisable type position.** `iter`, `rc`, `weak` and `shared` are ordinary identifiers, so they are sugar only after `->`, `:`, `<`, `,` inside `<..>`, `&` and similar. At the start of a group or after `,` outside `<..>` (tuple types, fn-pointer parameters) the plain Rust spelling is needed, because `f(rc x)` there is the expression sugar of item 16. A struct literal field looks like a typed one: `S { f: rc make() }` is a value because more than a type follows `rc`, but a bare name (`S { f: rc x }`) reads as a type and needs `S { f: (rc x) }`.
- **`rc e` values** need a value that cannot follow a name in valid Rust: `rc x`, `rc 0`, `rc vec![..]`, `rc move || ..`. A value starting with an operator or a delimiter (`rc &x`, `rc -1`, `rc (x)`, `rc |a| ..`) is valid Rust with `rc` as a name, so it stays. The value runs to the next `,` or `;` or the end of its group, like the operand of `return`: `rc a + b` wraps `a + b`.
- **Inline bounds decide "new generic or existing type" from the input only.** A name in scope gets the bound, a type declared or imported in the same input is an error, anything else becomes a new generic, so a glob-imported type is shadowed by a fresh generic.
- **`derive` presets are one flat namespace per `sugar!` input and match by written path.** A preset declared in a nested module is usable everywhere in the input, `Debug` and `std::fmt::Debug` are not recognised as the same derive, and derives inside macro bodies or nested `cfg_attr`s are not expanded.
- **`from` variants and `#[display]`** generate impls that copy the enum's generics without adding bounds, so a generic enum states what `Display` and `Error` need (`T: Display + Debug`). The generated `Error` has no `source()`, as in problem 05, and needs the enum to implement `Debug`. Duplicate `from` types are found by their spelling, so `io::Error` and `std::io::Error` meet only in rustc's conflicting-impls error. `from (A, B)` and `from ::a::B` read as a type named `from`; write `from std::..` or alias the tuple. Width and precision arguments (`{0:1$}`, `{:.*}`) are not supported in `#[display]`.
- **`#[newtype]` reads the validator's error type from its signature**, so `validate = f` needs `fn f(..) -> Result<(), E>` declared in the same item list as the struct. Its impls add no bounds (a generic newtype states `T: Display` itself), `deref` and `as_ref` lend `str` or `[T]` only for a type written `String` or `Vec<T>`, `display` forwards the formatter flags to the inner value, and operator flags cannot be combined with `validate`, because they would build unchecked values.
- **Attribute path cannot carry syntax sugar** (see d); only the function-like path supports the full vocabulary.
- **`#[dyn]` traits work only inside `sugar!`**, because rustc rejects the keyword path `#[dyn]` before an attribute macro sees the item. The generated `DynName` type is written out where `dyn Trait` would be (`&DynStore<'_>`, `DynStore::from_ref(&x)`); `dyn Trait` and `~Trait` are not rewritten to it. The expansion names `::rustx::dynosaur`, so the `rustx` dependency must not be renamed, and it needs the optional `dyn` feature of `rustx` (it brings `dynosaur` and its `syn` 2); without the feature `sugar!` reports `#[dyn]` itself as an error naming the feature.
- **An attribute macro sees only its item.** `#[delegate]` on a struct knows the std table and listed traits only, and cannot add `except(..)` items to a hand-written impl; `#[newtype(validate = f)]` cannot see `f`; `#[data(builder)]` cannot see a type its builder's name collides with; `#[borrows]` under `#[sugared]` reaches only the impls inside the same item. `#[sugared]` on an inline module sees the whole module. `#[from]` in an enum that derives `Error` or `From` is left to that derive.
- **Compile time:** `syn` with `full` features plus a parser adds build time to every user crate.
- **Semantics drift:** each sugar item must stay a pure, documented, local rewrite; anything that changes behaviour silently (e.g. lossy conversions) is out of scope.
