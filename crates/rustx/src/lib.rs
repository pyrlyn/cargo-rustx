//! cargo-rustx: a thin syntax layer over Rust that desugars into ordinary Rust.
//!
//! The facade users depend on: it re-exports the macros and holds the small runtime types
//! some sugar expands to, such as [`Shared`], so user crates need a single dependency.

mod rc;
mod shared;

/// The crate `#[dyn]` traits expand to, reached as `::rustx::dynosaur` so user crates need no
/// dependency of their own on it. Behind the `dyn` feature, because it brings its own `syn`.
#[cfg(feature = "dyn")]
pub use dynosaur;
pub use rc::RcExt;
pub use rustx_macros::{data, delegate, display, newtype, sugar, sugared};
pub use shared::Shared;

/// The error type behind `-> T throws`, which desugars to `::rustx::anyhow::Result<T>`; the
/// re-export spares users a dependency of their own and a version that could disagree.
pub use anyhow;
