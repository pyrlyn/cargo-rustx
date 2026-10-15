//! Procedural macros of cargo-rustx.
//!
//! A thin shell over `rustx-core`: a proc-macro crate cannot export anything else, and keeping
//! logic out of it lets the core be tested with plain `proc_macro2` tokens.

use proc_macro::TokenStream;
use rustx_core::{AttributeMacro, expand_attribute};

/// Desugars its body, a sequence of items written with cargo-rustx sugar, into plain Rust.
///
/// Valid Rust passes through unchanged.
#[proc_macro]
pub fn sugar(input: TokenStream) -> TokenStream {
    rustx_core::expand(input.into()).into()
}

/// Adds a constructor, getters and, with `#[data(setters, with, builder)]`, setters, `with_`
/// methods and a builder to a struct with named fields; `#[default(expr)]` on a field gives `new`
/// a default for it.
#[proc_macro_attribute]
pub fn data(args: TokenStream, item: TokenStream) -> TokenStream {
    expand_attribute(AttributeMacro::Data, args.into(), item.into()).into()
}

/// Implements `Display` and `Error` for an enum from the `#[display("..")]` on its variants, and
/// `From` for every variant whose single field carries `#[from]`.
#[proc_macro_attribute]
pub fn display(args: TokenStream, item: TokenStream) -> TokenStream {
    expand_attribute(AttributeMacro::Display, args.into(), item.into()).into()
}

/// Implements every trait named in a field's `#[delegate(Trait, ..)]` by forwarding to that field;
/// on a struct alone, only built-in std traits and traits given with their methods.
#[proc_macro_attribute]
pub fn delegate(args: TokenStream, item: TokenStream) -> TokenStream {
    expand_attribute(AttributeMacro::Delegate, args.into(), item.into()).into()
}

/// Adds `new` and one impl per flag to a one-field tuple struct: `deref`, `as_ref`, `display`,
/// `from_str`, `into` and the operators; `validate = f` needs `#[sugared]` on a module holding
/// both, since the macro cannot see `f`.
#[proc_macro_attribute]
pub fn newtype(args: TokenStream, item: TokenStream) -> TokenStream {
    expand_attribute(AttributeMacro::Newtype, args.into(), item.into()).into()
}

/// Applies every sugar that is valid Rust syntax inside the item: `#[data]`, `#[delegate(..)]`
/// fields, `#[display("..")]` and `#[from]`, `#[newtype(..)]`, `#[borrows]`.
///
/// Named `sugared` because `sugar!` already takes the name: macros of every kind share one
/// namespace.
#[proc_macro_attribute]
pub fn sugared(args: TokenStream, item: TokenStream) -> TokenStream {
    expand_attribute(AttributeMacro::Sugared, args.into(), item.into()).into()
}
