//! Desugaring core of cargo-rustx: turns the body of `sugar! { .. }` into plain Rust.
//!
//! Kept apart from the proc-macro crate so every pass runs on `proc_macro2` tokens and can be
//! unit- and snapshot-tested without a compiler. The CLI (`rustx-cli expand`) reuses it as is.
//!
//! Pipeline: token passes rewrite sugar that `syn` cannot parse into valid Rust, the result is
//! parsed as a [`syn::File`], then AST passes rewrite sugar that needs syntactic context.
//! Attribute macros ([`expand_attribute`]) get valid Rust and run AST passes only. With the
//! `sugar` feature, `sugar` converts plain Rust back into sugar.

mod attribute;
mod passes;
#[cfg(feature = "sugar")]
mod sugar;
mod tokens;

pub use attribute::{AttributeMacro, expand_attribute};
#[cfg(feature = "sugar")]
pub use sugar::{MacroTokens, sugar, without_trailing_commas};

use proc_macro2::TokenStream;
use quote::ToTokens;

/// Which optional features of the `rustx` facade the user crate has enabled.
///
/// Some sugar expands to code from an optional dependency of `rustx`; without it that sugar is
/// reported at its own tokens, naming the feature, instead of as an unresolved path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Features {
    /// The `dyn` feature: `#[dyn]` traits, which expand to `::rustx::dynosaur`.
    pub dyn_traits: bool,
}

impl Features {
    /// Every feature, for tools that show expansions rather than compile them.
    pub const ALL: Self = Self { dyn_traits: true };
}

/// Expands a `sugar!` body into Rust tokens; desugaring errors become `compile_error!` tokens.
#[must_use]
pub fn expand(input: TokenStream) -> TokenStream {
    expand_with(input, Features::ALL)
}

/// Like [`expand`], for a crate that has only `features` of `rustx` enabled.
#[must_use]
pub fn expand_with(input: TokenStream, features: Features) -> TokenStream {
    match try_expand_with(input, features) {
        Ok(file) => file.into_token_stream(),
        Err(err) => err.to_compile_error(),
    }
}

/// Expands a `sugar!` body into a parsed file, keeping errors as values for tests and tools.
///
/// # Errors
///
/// Returns every error a pass reports, combined, or the parse error when the desugared tokens
/// are not valid Rust.
pub fn try_expand(input: TokenStream) -> syn::Result<syn::File> {
    let tokens = passes::rewrite_tokens(input)?;
    let mut file: syn::File = syn::parse2(tokens)?;
    passes::rewrite_ast(&mut file)?;
    Ok(file)
}

/// Like [`try_expand`], with sugar that needs a disabled feature reported as well.
fn try_expand_with(input: TokenStream, features: Features) -> syn::Result<syn::File> {
    let missing = if features.dyn_traits {
        Ok(())
    } else {
        passes::reject_dyn(input.clone())
    };
    // Expanded anyway, so the other errors of the input are reported together with these.
    match (missing, try_expand(input)) {
        (Ok(()), expanded) => expanded,
        (Err(err), Ok(_)) => Err(err),
        (Err(mut err), Err(more)) => {
            err.combine(more);
            Err(err)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    #[test]
    fn valid_rust_passes_through_unchanged() {
        let input = quote! {
            fn add(a: i32, b: i32) -> i32 { a + b }
        };
        let file = try_expand(input.clone()).unwrap();
        assert_eq!(file.into_token_stream().to_string(), input.to_string());
    }

    #[test]
    fn dyn_without_its_feature_is_an_error_naming_the_feature() {
        let without = Features { dyn_traits: false };
        let messages = |input: TokenStream| -> Vec<String> {
            try_expand_with(input, without)
                .err()
                .into_iter()
                .flatten()
                .map(|err| err.to_string())
                .collect()
        };
        let needs = "`#[dyn]` needs the `dyn` feature of `rustx`: \
                     `rustx = { version = \"..\", features = [\"dyn\"] }`";
        assert_eq!(
            messages(quote! { mod m { #[dyn] pub trait T {} } }),
            [needs]
        );
        assert_eq!(
            messages(quote! { #[dyn] struct S; }),
            [
                needs,
                "`#[dyn]` goes on a trait, to make it usable as a dyn type"
            ]
        );
        let plain = quote! { trait T { async fn f(&self); } };
        assert!(try_expand_with(plain, without).is_ok());
        assert!(try_expand_with(quote! { #[dyn] trait T {} }, Features::ALL).is_ok());
    }

    #[test]
    fn invalid_rust_reports_a_parse_error() {
        let err = try_expand(quote! { fn broken() -> {} }).unwrap_err();
        assert!(err.to_string().starts_with("expected"), "{err}");
    }
}
