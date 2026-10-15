//! Procedural macros of cargo-rustx.
//!
//! A thin shell over `rustx-core`: a proc-macro crate cannot export anything else, and keeping
//! logic out of it lets the core be tested with plain `proc_macro2` tokens.

use proc_macro::TokenStream;

/// Desugars its body, a sequence of items written with cargo-rustx sugar, into plain Rust.
///
/// Valid Rust passes through unchanged.
#[proc_macro]
pub fn sugar(input: TokenStream) -> TokenStream {
    rustx_core::expand(input.into()).into()
}
