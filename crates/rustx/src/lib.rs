//! cargo-rustx: a thin syntax layer over Rust that desugars into ordinary Rust.
//!
//! The facade users depend on: it re-exports the macros and holds the small runtime types
//! some sugar expands to, such as [`Shared`], so user crates need a single dependency.

mod shared;

pub use rustx_macros::{data, delegate, display, newtype, sugar, sugared};
pub use shared::Shared;
