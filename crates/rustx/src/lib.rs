//! cargo-rustx: a thin syntax layer over Rust that desugars into ordinary Rust.
//!
//! The facade users depend on: it re-exports the macros and will hold the small runtime
//! helpers some sugar expands to, so user crates need a single dependency.

pub use rustx_macros::{data, delegate, display, sugar, sugared};
