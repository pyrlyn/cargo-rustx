//! Desugaring core of cargo-rustx: turns the body of `sugar! { .. }` into plain Rust.
//!
//! Kept apart from the proc-macro crate so every pass runs on `proc_macro2` tokens and can be
//! unit- and snapshot-tested without a compiler. A future CLI (`rustx expand`) reuses it as is.
//!
//! Pipeline: token passes rewrite sugar that `syn` cannot parse into valid Rust, the result is
//! parsed as a [`syn::File`], then AST passes rewrite sugar that needs syntactic context.

mod passes;
mod tokens;

use proc_macro2::TokenStream;
use quote::ToTokens;

/// Expands a `sugar!` body into Rust tokens; desugaring errors become `compile_error!` tokens.
#[must_use]
pub fn expand(input: TokenStream) -> TokenStream {
    match try_expand(input) {
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
    fn invalid_rust_reports_a_parse_error() {
        let err = try_expand(quote! { fn broken() -> {} }).unwrap_err();
        assert!(err.to_string().starts_with("expected"), "{err}");
    }
}
