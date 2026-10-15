# Toolchain

| Program | How to install | Why here | Source |
| --- | --- | --- | --- |
| rustc, cargo | rustup (`rust-toolchain.toml`) | Build | https://github.com/rust-lang/rust |
| cargo-nextest | cargo install / taiki-e/install-action in CI | Test runner | https://github.com/nextest-rs/nextest |
| just | cargo install / brew | Task runner (`just check`) | https://github.com/casey/just |

## cargo

| Package | Where | Source | Why here |
| --- | --- | --- | --- |
| proc-macro2 | local | https://github.com/dtolnay/proc-macro2 | Tokens outside the compiler, so the core is testable; `span-locations` in `rustx-cli` for error lines and columns, and through the `rustx-core` `sugar` feature for byte offsets of edits |
| quote | local | https://github.com/dtolnay/quote | Building output tokens |
| syn | local | https://github.com/dtolnay/syn | Parsing the desugared body and AST passes; `visit` (through the `rustx-core` `sugar` feature) for reverse rules |
| clap | local | https://github.com/clap-rs/clap | `rustx-cli` arguments and subcommands (derive) |
| dynosaur | local (optional, `rustx` feature `dyn`) | https://github.com/spastorino/dynosaur | `#[dyn]` traits expand to its attribute; re-exported by `rustx` only with `dyn`, because it pulls in `syn` 2 for its own macro |
| prettyplease | local | https://github.com/dtolnay/prettyplease | Formatting `rustx-cli expand` output and readable expansions in snapshots |
| anyhow | local | https://github.com/dtolnay/anyhow | Error type of `-> T throws`, re-exported by `rustx` |
| insta | local (dev) | https://github.com/mitsuhiko/insta | Snapshot tests of expansions |
| trybuild | local (dev) | https://github.com/dtolnay/trybuild | Compile-pass and compile-fail tests |
