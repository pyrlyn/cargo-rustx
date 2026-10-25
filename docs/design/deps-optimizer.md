# Design: deps optimizer — a cargo wrapper that keeps `target/*/deps` small

Status: draft, design only. Nothing here is implemented; no files on any machine are removed by this PR.

## 1. Problem

Measured on Ivan's Mac on 2026-10-21 (926 GB volume, ~2 GB free at the time):

| Where | Size | Dirs |
|---|---|---|
| All build dirs | 208 GB | 783 |
| of which cargo `target/` | ~190 GB | |
| Claude Code worktree targets (`.claude/worktrees/*`) | 109 GB | 16 |
| Our `-wt` / `.tokcli/worktrees` targets | 44.5 GB | 11 |
| `/tmp` targets | 32 GB | 648 |
| Xcode CoreSimulator (not cargo) | 16 GB | |
| sccache (default 10 GB cap, full) | 10 GB | |

Inside the 21 biggest targets:

| Part | Size |
|---|---|
| `debug/` total | 152 GB |
| `debug/deps` | 124 GB |
| `debug/incremental` | 22 GB |
| `debug/build` (build scripts) | 6.3 GB |
| test artifacts | 8.5 GB |
| `aarch64-apple-darwin/` (explicit `--target`) | 4.4 GB |
| `release/` | ≈ 0 |

There are 11,046 `.rlib` files. The same crates are built again and again:
`toml_datetime` 95×, `syn` 90×, `toml` 85×, `hashbrown` 78×, `getrandom` 78×, `thiserror` 75×, `winnow` 70×, `darling_core` 64×.

Root cause: every worktree has its own `target/`, so every worktree compiles the full dependency graph from scratch, and small differences in features, profile or `RUSTFLAGS` give each copy a different metadata hash, so even a shared dir would keep several copies.

## 2. What should live in `deps/`

Short answer: **in a shared dir, `deps/` should hold only reusable compiled dependency outputs — `.rlib`, `.rmeta`, and proc-macro `.dylib`s — one copy per (package, version, features, profile, rustflags, target).** Everything else is either project-specific or throwaway and should be kept elsewhere or not kept at all.

What actually sits in `target/<profile>/deps` today:

| File | What | Reusable across worktrees? |
|---|---|---|
| `lib<name>-<hash>.rlib` | compiled library | **Yes**, if the hash matches |
| `lib<name>-<hash>.rmeta` | metadata for check/pipelining | **Yes** |
| `lib<name>-<hash>.dylib` (proc-macro) | compiled proc macro, loaded by rustc | **Yes**, host-only; cannot be cached by sccache |
| `<name>-<hash>.d` | dep-info (Makefile-style, absolute paths) | Small; tied to the source paths. Keep, but it is bookkeeping, not an artifact |
| `<test>-<hash>` executables | unit/integration test binaries | **No**, workspace-specific, big (debug info), regenerable |
| workspace crates' own rlibs | our code | **No**, changes every edit |

Outside `deps/` in the same profile dir:

- `build/<pkg>-<hash>/` — build-script binaries and their `OUT_DIR`. Third-party ones are reusable like rlibs; workspace ones are project-specific.
- `incremental/` — per-crate incremental session data. Only useful for workspace crates of one checkout; worthless for deps (deps are never incremental) and not cached by sccache.
- `.fingerprint/` — small, needed for freshness.
- final binaries in `<profile>/` — project output.

## 3. How to get there

1. **One target dir per repo pool, not per worktree.** Point `CARGO_TARGET_DIR` at `~/.cache/cargo-target/<repo>/<slot>` so third-party deps are compiled once per slot.
2. **Split intermediate from final with `build-dir`.** Cargo's `build.build-dir` (stabilized in recent Cargo; `-Zbuild-dir` before that) moves intermediate artifacts (`deps`, `build`, `incremental`, `.fingerprint`) out of `target-dir`. The shared pool becomes the build-dir; each worktree keeps a tiny local `target-dir` for its final binaries, so tests that look in `target/debug` keep working.
3. **Make the hashes match.** The metadata hash covers package id + enabled features + profile settings + `RUSTFLAGS` + target triple (+ rustc version). To collapse 95 copies into 1–2:
   - workspace-hack crate via `cargo hakari` to unify features across the workspace;
   - `resolver = "2"` (or edition 2024 default) everywhere;
   - one `[profile.dev]` definition per repo, no per-worktree overrides;
   - one `RUSTFLAGS` set by the wrapper; drop ad hoc `--target aarch64-apple-darwin` on host builds (that alone is a 4.4 GB second tree);
   - one pinned toolchain (`rust-toolchain.toml`).
4. **sccache for cross-repo / cross-slot reuse.** `RUSTC_WRAPPER=sccache` reuses rlibs between pools and repos. It does not cache proc-macro crates, crates with `cdylib`/`bin` outputs, or incremental builds — hence `CARGO_INCREMENTAL=0` for agents.
5. **Shrink each artifact.** `debug = "line-tables-only"` for workspace code and `debug = false` for dependencies; debug info is the bulk of rlib size.

## 4. Components

### 4.1 `cargo` shim (wrapper)
A `cargo` binary earlier on `PATH` (or `cargo rx-build` subcommand) that:
- resolves the repo (git common dir, so all worktrees map to one repo key);
- asks the daemon for a target slot lease;
- exports `CARGO_BUILD_BUILD_DIR`, `CARGO_INCREMENTAL=0`, `RUSTC_WRAPPER=sccache`, normalized `RUSTFLAGS`;
- execs the real cargo; releases the lease on exit.

### 4.2 Daemon
A small long-running process (launchd agent) holding slot state, leases with heartbeats, and the cleanup scheduler. Without the daemon the shim falls back to file locks (`flock`) on the slot dirs.

### 4.3 Target pool
- 2–3 slots per repo: `~/.cache/cargo-target/<repo>/{0,1,2}`.
- Selection: first free slot that cargo's own `.package-cache`/build lock is not held on; prefer the slot last used by the same branch (warm workspace crates).
- If all slots are busy: wait (with timeout) rather than create a new slot.
- Lease expires after missed heartbeats so a crashed agent does not pin a slot.

### 4.4 Disk guard
Before granting a lease: if free space < 30 GB, run the cleanup policy once; if still < 30 GB, refuse the job with a clear error. Agents/CI treat that as "retry later".

### 4.5 Cleanup policy
- `cargo sweep --time 3` on every slot daily (artifacts unused for 3 days).
- Delete a worktree's local target-dir right after its PR branch is pushed.
- Delete `/tmp/*` cargo targets older than 1 day.
- Cap: at most 5 worktrees per repo; the oldest merged one is removed first.
- `xcrun simctl delete unavailable` weekly (CoreSimulator, 16 GB).
- Never touch a slot that holds a live lease.

### 4.6 Feature normalizer
Runs `cargo hakari generate` (or checks it is up to date) and reports packages that still compile in more than N variants by scanning `deps/` for repeated `lib<name>-*.rlib`. Output is a report, not an auto-fix.

## 5. Configs

`.cargo/config.toml` (per repo, committed):

```toml
[build]
# intermediate artifacts go to the shared pool; set per slot by the shim
# build-dir = "{cargo-cache-home}/build/{workspace-path-hash}"
incremental = false

[profile.dev]
debug = "line-tables-only"

[profile.dev.package."*"]
debug = false
```

(The `[profile]` part can also live in `Cargo.toml`.)

Environment set by the shim:

```sh
export CARGO_TARGET_DIR="$HOME/.cache/cargo-target/$REPO/$SLOT"   # or CARGO_BUILD_BUILD_DIR with a local target-dir
export CARGO_INCREMENTAL=0
export RUSTC_WRAPPER=sccache
export SCCACHE_CACHE_SIZE=15G
```

Commands:

```sh
cargo install cargo-hakari cargo-sweep
cargo hakari init workspace-hack && cargo hakari generate && cargo hakari manage-deps
cargo sweep --time 3 ~/.cache/cargo-target/tokcli/0
sccache --show-stats
# duplicates report
find ~/.cache/cargo-target -name 'lib*.rlib' | sed -E 's#.*/lib(.+)-[0-9a-f]+\.rlib#\1#' | sort | uniq -c | sort -rn | head
```

## 6. Savings estimate (estimates, not measurements)

- **Shared deps per repo**: 27 worktree targets (109 + 44.5 GB) → ~3 slots per repo for ~8 repos. Third-party deps dominate `deps/` (124 GB); with 2–3 copies per repo instead of one per worktree we expect `deps` to drop by ~80–90 % → ~110 GB saved.
- **`CARGO_INCREMENTAL=0`**: −22 GB.
- **Debug-info trimming**: rlibs typically shrink 2–3× → further ~50 % of what remains in deps.
- **`/tmp` targets**: 32 GB, all regenerable, kept ≤ 1 day.
- **Test executables**: 8.5 GB, shrink with line-tables-only.
- Expected steady state for all cargo build dirs: **~20–35 GB instead of ~190 GB**, plus sccache bounded at 15 GB.

## 7. Rollout

1. Profile settings + `CARGO_INCREMENTAL=0` in each repo (no tooling needed).
2. `cargo hakari` workspace-hack in the biggest repos (the token CLI, the package manager, kerf, crates).
3. Shim with file-lock pool, no daemon; enable for agents only.
4. Disk guard + cleanup policy as a launchd job.
5. Daemon with leases; then CI self-hosted runners.
6. Measure duplicates report weekly; tune slot count.

## 8. Risks

- **Lock contention**: cargo locks the whole target dir; parallel agents on one slot serialize. Mitigated by 2–3 slots and lease-aware selection.
- **sccache limits**: does not cache incremental builds, proc macros, or linking; cold-cache first builds still pay full cost.
- **Proc macros** are host dylibs; a different host toolchain or flags rebuilds them all.
- **Hash churn**: any feature/flag/toolchain drift silently creates new copies; the normalizer report is the guard.
- **`build-dir` availability** depends on Cargo version; fallback is a plain shared `CARGO_TARGET_DIR`, which changes where tests find binaries.
- Cleanup deleting a target in active use; prevented by honoring leases.
