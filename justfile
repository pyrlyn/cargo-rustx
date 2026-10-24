# Task runner for cargo-rustx. `just check` is the gate before every commit.

msrv := "1.97"

default: check

# fmt, clippy (all targets, warnings are errors), tests, MSRV
check: fmt-check clippy test msrv

fmt-check:
    cargo fmt --all --check
    cargo fmt --manifest-path boilerless/Cargo.toml --all --check

clippy:
    cargo clippy --workspace --all-targets -- -D warnings
    cargo clippy --manifest-path boilerless/Cargo.toml --workspace --all-targets -- -D warnings

test:
    cargo nextest run --workspace
    cargo test --workspace --doc
    cargo test --manifest-path boilerless/Cargo.toml --workspace
    python3 -m unittest discover boilerless/tools/ra-macros/tests

# The lowest supported toolchain must still build the crates.
msrv:
    cargo +{{msrv}} check --workspace

# Accept new or changed snapshots and compile-fail messages; review the diff.
bless:
    INSTA_UPDATE=always TRYBUILD=overwrite cargo nextest run --workspace
