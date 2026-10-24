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
