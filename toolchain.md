# Toolchain

| Program | How to install | Why here | Source |
| --- | --- | --- | --- |
| rustc, cargo | rustup (`rust-toolchain.toml`) | Build | https://github.com/rust-lang/rust |
| cargo-nextest | cargo install / taiki-e/install-action in CI | Test runner | https://github.com/nextest-rs/nextest |
| just | cargo install / brew | Task runner (`just check`) | https://github.com/casey/just |

## cargo

| Package | Where | Source | Why here |
| --- | --- | --- | --- |
| proc-macro2 | local | https://github.com/dtolnay/proc-macro2 | Tokens outside the compiler, so the core is testable |
| quote | local | https://github.com/dtolnay/quote | Building output tokens |
| syn | local | https://github.com/dtolnay/syn | Parsing the desugared body and AST passes |
| insta | local (dev) | https://github.com/mitsuhiko/insta | Snapshot tests of expansions |
| prettyplease | local (dev) | https://github.com/dtolnay/prettyplease | Readable expansions in snapshots |
| trybuild | local (dev) | https://github.com/dtolnay/trybuild | Compile-pass and compile-fail tests |
