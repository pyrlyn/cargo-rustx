//! Registry of desugaring passes and the order they run in.
//!
//! One module per sugar item keeps passes independent; only this file knows the order.
//! Token passes run first, in list order, because their output must parse as Rust.
//! AST passes then run on the parsed file; all of them run so errors are reported together.

mod attr_flags;
mod borrows;
mod bound_presets;
mod call_defaults;
mod capture_lists;
mod data;
mod delegate;
mod derive_presets;
mod dyn_traits;
mod field_defaults;
mod from_attribute;
mod from_display;
mod from_lifetimes;
mod impl_header;
mod inline_bounds;
mod into_params;
mod item_lists;
mod macro_args;
mod newtype;
mod param_defaults;
mod prefix_types;
mod rc_values;
mod shared;
mod signatures;
mod throw;
mod throws;
mod turbofish;
mod typed_positions;
mod var;
mod widen;

use proc_macro2::TokenStream;

use crate::AttributeMacro;
use crate::tokens::syntax::combine;

type TokenPass = fn(TokenStream) -> syn::Result<TokenStream>;
type AstPass = fn(&mut syn::File) -> syn::Result<()>;

// `bound` presets first so later passes see ordinary traits; inline bounds and `from` lifetimes
// before `prefix_types`, so `iter T` can pick up a `from` lifetime instead of `'_`. Item-level
// M4 sugar (`derive` presets, `from` variants) right after `bound` presets. Named
// arguments before `from` lifetimes and `prefix_types`, which would read `name:` as a type's `:`.
// Capture lists last: they read the closure after them with `syn`, which needs every other
// sugar in it gone.
// `throws` after `prefix_types`: moved into `Result<..>` first, `iter &T` would sit after `<`,
// where `prefix_types` cannot tell it from a comparison.
// `throw` right after `throws`, whose body marker tells it the fn's error type, and before
// capture lists, which parse the closure after a list with `syn`.
const TOKEN_PASSES: &[TokenPass] = &[
    var::rewrite,
    shared::rewrite,
    bound_presets::rewrite,
    derive_presets::rewrite,
    dyn_traits::rewrite,
    from_display::rewrite,
    inline_bounds::rewrite,
    into_params::rewrite,
    param_defaults::rewrite,
    call_defaults::rewrite,
    from_lifetimes::rewrite,
    prefix_types::rewrite,
    throws::rewrite,
    throw::rewrite,
    rc_values::rewrite,
    turbofish::rewrite,
    capture_lists::rewrite,
];

// Derive-like M4 passes first, so the impls they generate get generics and lifetimes from
// `inline_bounds` and `#[borrows]`. `#[data]` before field defaults, which take off the
// `= value` that `new` and the builder read. `#[borrows]` before `from` lifetimes, so `from(Self)` finds
// the type's lifetime. Call-site defaults before `param_defaults::lower`, which takes off the
// markers they read.
const AST_PASSES: &[AstPass] = &[
    derive_presets::lower,
    data::lower,
    field_defaults::lower,
    from_display::lower,
    newtype::lower,
    delegate::lower,
    inline_bounds::lower,
    into_params::lower,
    call_defaults::lower,
    param_defaults::lower,
    borrows::lower,
    from_lifetimes::lower,
    dyn_traits::lower,
    shared::lower,
    widen::lower,
    throws::lower,
];

pub(crate) use delegate::{EXCEPT_WITHOUT_IMPL, UNKNOWN_TRAIT};
pub(crate) use newtype::VALIDATOR_NOT_FOUND;

pub(crate) fn rewrite_tokens(mut tokens: TokenStream) -> syn::Result<TokenStream> {
    for pass in TOKEN_PASSES {
        tokens = pass(tokens)?;
    }
    Ok(tokens)
}

/// Reports every `#[dyn]` in `tokens`, for a crate without the `dyn` feature of `rustx`.
pub(crate) fn reject_dyn(tokens: TokenStream) -> syn::Result<()> {
    dyn_traits::reject(tokens)
}

pub(crate) fn rewrite_ast(file: &mut syn::File) -> syn::Result<()> {
    run_ast(AST_PASSES, file)
}

/// Runs the AST passes behind one attribute macro on the item it sits on, rebuilt as a file.
///
/// `#[from]` is read only by the attribute macros: inside `sugar!` the spelling is `from T`, and a
/// `#[from]` there stays valid Rust for whatever derive owns it. It runs first, so that
/// `from_display` finds its marker.
pub(crate) fn rewrite_attribute(kind: AttributeMacro, file: &mut syn::File) -> syn::Result<()> {
    let (first, passes): (&[AstPass], &[AstPass]) = match kind {
        AttributeMacro::Data => (&[], &[data::lower]),
        AttributeMacro::Display => (&[from_attribute::lower], &[from_display::lower]),
        AttributeMacro::Delegate => (&[], &[delegate::lower]),
        AttributeMacro::Newtype => (&[], &[newtype::lower]),
        AttributeMacro::Sugared => (&[from_attribute::lower], AST_PASSES),
    };
    run_ast(first.iter().chain(passes), file)
}

fn run_ast<'a>(
    passes: impl IntoIterator<Item = &'a AstPass>,
    file: &mut syn::File,
) -> syn::Result<()> {
    let mut errors: Option<syn::Error> = None;
    for pass in passes {
        if let Err(err) = pass(file) {
            combine(&mut errors, err);
        }
    }
    errors.map_or(Ok(()), Err)
}
