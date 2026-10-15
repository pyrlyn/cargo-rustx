# Roadmap

Approved milestones from `ARCHITECTURE.md` (g), not yet in `plan.md`.

- M4 derives: `#[data]` (08), `from` variants and `#[display]` (05), `#[delegate]` (07), `derive` presets (18), `#[newtype]` (20), field defaults (17); also as standalone attribute macros.
- M5 errors: `T throws`, `throw`, tail `Ok(..)` (10).
- M6 closures and sharing: `move[..]` (11), `shared` and `Shared<T>` (12), `rc e` and `rc` field access (16).
- M7 async: `async fn` in traits, boxed for `dyn` use (14).
- M8 conversions: implicit `into()` in syntactically typed positions (13), string literals into `String` (15).
- M9 tooling: `rustx-cli expand`/`fmt`, file-level `.rustx` via build.rs, docs.
