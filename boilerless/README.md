# boilerless

One derive for common Rust boilerplate: builders, getters/setters, constructors, error enums,
enum <-> string, newtype conversion and delegation, variant registries, env config, and
`#[bind(Trait)]` for method -> trait binding.

```rust
use boilerless::Boil;

#[derive(Boil)]
#[boil(builder, getters)]
struct Server { host: String, #[boil(default = 8080)] port: u16 }
```

See `boilerless/src/lib.rs` for the pattern table, `boilerless/examples/showcase.rs` for a tour
and `docs/patterns.md` for the research notes behind the design.

## Layout

- `macros/`: proc-macro crate (parse -> rules -> codegen)
- `boilerless/`: facade crate, examples, tests (`tests/ui` compile-fail, `tests/e2e` end-to-end)
- `tools/ra-macros/`: rust-analyzer proc-macro switcher
- `docs/patterns.md`: research write-up

## Tests

```sh
cd boilerless && cargo test && cargo clippy --all-targets
python3 -m unittest discover tools/ra-macros/tests
```

## rust-analyzer proc-macro switcher

```sh
python3 tools/ra-macros/ra_macros.py status|on|off [PROJECT_DIR]
```

Edits `PROJECT_DIR/.vscode/settings.json` (default: current dir). Idempotent, keeps other keys,
backs up once and refuses JSONC with comments. Details in `tools/ra-macros/README.md`.

## Conditional and generic delegation

`#[bind(Trait)]` keeps `cfg` and `cfg_attr` conditions on forwarded methods and passes
method type and const parameters explicitly. Late-bound lifetimes remain inferred.
The runnable examples `boilerless/tests/e2e/bind_conditions.rs` and
`boilerless/tests/e2e/bind_generics.rs` cover both cases.

```rust
trait Measure { fn size<T, const N: usize>(&self) -> usize; }
struct Meter;
#[boilerless::bind(Measure)]
impl Meter {
    fn size<T, const N: usize>(&self) -> usize { std::mem::size_of::<T>() * N }
}
```

## Skipped generic builder fields

`#[boil(skip)]` fields use their explicit default or `Default::default()`. They may be the
only fields using a type or lifetime parameter; the generated builder retains those
parameters without requiring it to own the skipped values. An internal marker never
replaces a user field. See `boilerless/tests/e2e/builder_skipped_generics.rs` for a runnable
example with a skipped type, a lifetime, a const parameter, and a colliding field name.

From the repository root, `just check` checks both workspaces and the Python switcher tests.

## Raw identifiers

The `r#` escape is syntax, not part of generated strings: a config field `r#type` with prefix
`"APP_"` reads `APP_TYPE`; an enum variant `r#type` has `as_str()` and `variant_name()` equal
to `"type"`. An explicit `#[boil(rename = "r#literal")]` keeps the exact requested string.
See `boilerless/tests/e2e/raw_identifiers.rs` for a runnable example covering all three patterns.
