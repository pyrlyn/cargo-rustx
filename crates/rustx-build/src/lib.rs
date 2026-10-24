//! Desugars cargo-rustx sources outside the compiler.
//!
//! From `build.rs`, [`expand_file`] and [`expand_dir`] turn `.rustx` files, which are sugar
//! throughout, into plain Rust under `OUT_DIR` for `include!`:
//!
//! ```no_run
//! // In `fn main` of build.rs:
//! if let Err(err) = rustx_build::expand_file("src/stats.rustx") {
//!     err.emit();
//! }
//! ```
//!
//! and in `src/lib.rs`: `include!(concat!(env!("OUT_DIR"), "/src/stats.rs"));`.
//!
//! [`expand_source`] and [`render`] are the expansion and error rendering behind
//! `rustx-cli expand`.

mod diagnostic;
mod expand;
mod files;

pub use diagnostic::render;
pub use expand::{Kind, expand_source, is_sugar};
pub use files::{Error, expand_dir, expand_file};
