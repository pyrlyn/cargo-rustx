//! Proc-macros behind `boilerless`.
//!
//! The pipeline is split in three layers:
//!
//! 1. [`parse`]: `#[boil(...)]` attributes -> typed configs ([`parse::Pattern`], field/variant configs).
//! 2. [`rules`]: the declarative registry of every pattern (name, description, accepted
//!    syntax, accepted item shape, what it generates) plus shape validation.
//! 3. [`codegen`]: one module per pattern turning a validated config into tokens.
//!
//! The public entry points (`#[derive(Boil)]`, `#[bind(Trait)]`) only wire these together.

mod codegen;
mod parse;
mod rules;

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use syn::DeriveInput;

/// Derive one or more boilerplate patterns, selected by `#[boil(...)]` on the item.
///
/// See the `boilerless` crate docs for the list of patterns.
#[proc_macro_derive(Boil, attributes(boil))]
pub fn derive_boil(input: TokenStream) -> TokenStream {
    let input = syn::parse_macro_input!(input as DeriveInput);
    expand_derive(&input)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// Bind the methods of an inherent `impl` block to a trait: `#[bind(Trait)] impl Foo { ... }`
/// keeps the inherent block and adds `impl Trait for Foo` delegating every method to it.
#[proc_macro_attribute]
pub fn bind(attr: TokenStream, item: TokenStream) -> TokenStream {
    let attr: TokenStream2 = attr.into();
    let item: TokenStream2 = item.into();
    codegen::bind::expand(attr, item)
        .unwrap_or_else(syn::Error::into_compile_error)
        .into()
}

/// parse -> rules -> codegen for `#[derive(Boil)]`.
pub(crate) fn expand_derive(input: &DeriveInput) -> syn::Result<TokenStream2> {
    let config = parse::parse_container(input)?;
    if config.patterns.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.ident,
            format!(
                "#[derive(Boil)] needs at least one pattern, e.g. #[boil(builder)]; available: {}",
                rules::names()
            ),
        ));
    }
    let mut out = TokenStream2::new();
    for pattern in &config.patterns {
        let rule = rules::lookup(pattern.name()).expect("every parsed pattern has a rule");
        rules::check(rule, input)?;
        out.extend(codegen::expand(pattern, input)?);
    }
    Ok(out)
}
