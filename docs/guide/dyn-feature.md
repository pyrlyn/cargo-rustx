# The `dyn` feature

[`#[dyn]` traits](dyn-traits.md) expand to the `dynosaur` crate, which `rustx` re-exports. It brings its own `syn`, so it sits behind an optional feature that crates without `#[dyn]` do not compile:

```toml
[dependencies]
rustx = { version = "..", features = ["dyn"] }
```

Without the feature, `sugar!` reports each `#[dyn]` as an error that names the feature, instead of an unresolved path in the expansion. The expansion names `::rustx::dynosaur`, so keep the dependency's name `rustx`.

The guide's own examples are compiled with the feature on: the `rustx` crate turns it on for its tests through a dev-dependency on itself.
