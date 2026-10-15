//! Desugars cargo-rustx sources outside the compiler: the expansion and error rendering behind
//! `rustx-cli expand`.

mod diagnostic;
mod expand;

pub use diagnostic::render;
pub use expand::{Kind, expand_source};
