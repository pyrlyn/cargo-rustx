//! Parameter defaults `port: u16 = 8080` → `Option<u16>` plus `unwrap_or` (problem 17,
//! parameters only; field defaults belong to M4).
//!
//! Two halves, because the sugar does not parse but the lowering needs the parsed fn:
//!
//! 1. Token pass ([`marker`]): in the parameter list of `fn name<..>(..)`, a parameter with a
//!    top-level `=` (`x: T = e`) becomes `#[__rustx_default(e)] x: T`, which `syn` parses.
//! 2. AST pass ([`lower`]): every fn with a body takes its markers off, wraps each defaulted
//!    type in `Option` and starts the body by unwrapping it, in parameter order:
//!
//! ```text
//! fn connect(host: &str, port: u16 = 5432, name: String = host.to_owned()) { .. }
//! fn connect(
//!     host: &str,
//!     port: ::core::option::Option<u16>,
//!     name: ::core::option::Option<String>,
//! ) {
//!     let port: u16 = port.unwrap_or(5432);
//!     let name: String = name.unwrap_or_else(|| host.to_owned());
//!     ..
//! }
//! ```
//!
//! # Rules
//!
//! - **Eager or lazy.** A literal, a path, or arithmetic, casts, tuples and arrays of those is
//!   passed to `unwrap_or`; anything else (calls, macros, blocks) goes into
//!   `unwrap_or_else(|| ..)`, so an expensive default runs only when the argument is `None`.
//! - **Trailing only.** Once a parameter has a default, every later one needs one too, so a call
//!   that leaves arguments out (T10.2) always drops a suffix and positional arguments keep their
//!   meaning. `self` takes no default.
//! - **Bodies only.** A trait method without a default body or a foreign fn cannot apply a
//!   default, so a default there is an error.
//! - A default may use the parameters before it: they are already unwrapped when it runs.
//! - A plain binding (`port`, `mut port`) keeps its name in the signature; any other pattern
//!   moves into the `let`, and the parameter is renamed `__rustx_default_<index>`.
//!
//! # Finding the defaults later (call-site sugar, T10.2)
//!
//! Until [`lower`] runs, every defaulted parameter carries its marker. A pass that needs the
//! defaults of a fn (to fill omitted arguments or reorder named ones) runs before [`lower`] in
//! `AST_PASSES` and reads each parameter with [`default_of`]; the marker is the contract, not
//! the lowered `Option` form, which hand-written Rust can look like too.
//!
//! # Limitations
//!
//! - The shared parameter splitter counts `<` as an open generic, so a comparison or shift in a
//!   default (`ok: bool = a < b`) must be parenthesised; the pass reports it.
//! - A non-constant default runs inside a closure: `?`, `return` and `.await` there act on the
//!   closure, and rustc reports them.
//! - A default on an `impl Trait` or generic parameter must produce that type, which is rarely
//!   possible beyond `Default::default()`; callers passing `None` may need to name the type.
//! - Signatures inside macro bodies are not lowered: the marker stays and rustc reports the
//!   unknown attribute `__rustx_default` at the `=`.

mod lower;
mod marker;

use proc_macro2::{Span, TokenStream};
use syn::{Attribute, Expr};

#[cfg(feature = "sugar")]
pub(crate) use lower::{has_impl_trait, is_constant};
#[cfg(feature = "sugar")]
pub(crate) use marker::MARKER;

/// The default the token half put on a parameter: the span of its `=` and the expression.
pub(super) struct ParamDefault {
    pub(super) eq: Span,
    pub(super) expr: syn::Result<Expr>,
}

/// The default of the parameter with these attributes, read from its marker without removing
/// it; `None` when the parameter has none. Only meaningful before [`lower`] runs.
pub(super) fn default_of(attrs: &[Attribute]) -> Option<ParamDefault> {
    let marker = attrs
        .iter()
        .find(|attr| attr.path().is_ident(marker::MARKER))?;
    Some(ParamDefault {
        eq: marker.pound_token.spans[0],
        expr: marker.parse_args(),
    })
}

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    marker::rewrite(tokens)
}

pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    lower::lower(file)
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::{ToTokens, quote};

    fn desugar(sugar: TokenStream) -> syn::Result<String> {
        let mut file: syn::File = syn::parse2(rewrite(sugar)?)?;
        lower(&mut file)?;
        Ok(file.into_token_stream().to_string())
    }

    fn reprint(plain: TokenStream) -> String {
        syn::parse2::<syn::File>(plain)
            .map(|file| file.into_token_stream().to_string())
            .unwrap_or_default()
    }

    fn errors(sugar: TokenStream) -> Vec<String> {
        desugar(sugar)
            .err()
            .into_iter()
            .flatten()
            .map(|err| err.to_string())
            .collect()
    }

    #[test]
    fn a_constant_default_becomes_option_and_unwrap_or() {
        assert_eq!(
            desugar(quote! {
                pub fn connect(host: &str, port: u16 = 5432) -> String { format!("{host}:{port}") }
            })
            .ok(),
            Some(reprint(quote! {
                pub fn connect(host: &str, port: ::core::option::Option<u16>) -> String {
                    let port: u16 = port.unwrap_or(5432);
                    format!("{host}:{port}")
                }
            }))
        );
    }

    #[test]
    fn a_non_constant_default_is_lazy_and_sees_earlier_parameters() {
        assert_eq!(
            desugar(quote! { fn f(a: u8 = 1, name: String = a.to_string()) {} }).ok(),
            Some(reprint(quote! {
                fn f(a: ::core::option::Option<u8>, name: ::core::option::Option<String>) {
                    let a: u8 = a.unwrap_or(1);
                    let name: String = name.unwrap_or_else(|| a.to_string());
                }
            }))
        );
    }

    #[test]
    fn mut_moves_into_the_body_and_patterns_get_a_generated_name() {
        assert_eq!(
            desugar(quote! { fn f(mut n: u8 = 0, (a, b): (u8, u8) = (1, 2)) {} }).ok(),
            Some(reprint(quote! {
                fn f(
                    n: ::core::option::Option<u8>,
                    __rustx_default_1: ::core::option::Option<(u8, u8)>
                ) {
                    let mut n: u8 = n.unwrap_or(0);
                    let (a, b): (u8, u8) = __rustx_default_1.unwrap_or((1, 2));
                }
            }))
        );
    }

    #[test]
    fn methods_trait_defaults_and_nested_fns_are_lowered() {
        assert_eq!(
            desugar(quote! {
                impl S { fn m(&self, x: u8 = 1) { fn n(y: u8 = 2) {} } }
                trait T { fn t(&self, z: u8 = 3) {} }
            })
            .ok(),
            Some(reprint(quote! {
                impl S {
                    fn m(&self, x: ::core::option::Option<u8>) {
                        let x: u8 = x.unwrap_or(1);
                        fn n(y: ::core::option::Option<u8>) { let y: u8 = y.unwrap_or(2); }
                    }
                }
                trait T {
                    fn t(&self, z: ::core::option::Option<u8>) { let z: u8 = z.unwrap_or(3); }
                }
            }))
        );
    }

    #[test]
    fn an_impl_trait_parameter_is_unwrapped_without_an_annotation() {
        assert_eq!(
            desugar(quote! { fn f(s: impl Default = Default::default()) {} }).ok(),
            Some(reprint(quote! {
                fn f(s: ::core::option::Option<impl Default>) {
                    let s = s.unwrap_or_else(|| Default::default());
                }
            }))
        );
    }

    #[test]
    fn valid_rust_passes_through_unchanged() {
        let valid = quote! {
            fn f<const N: usize = 3>(i: impl Iterator<Item = u8>, p: Option<u8>) -> bool {
                let x = p.unwrap_or(1);
                let c = |y: u8| y == x;
                i.count() == N
            }
            struct S { a: u8 }
            trait T { fn t(&self, a: u8); }
        };
        assert_eq!(desugar(valid.clone()).ok(), Some(reprint(valid)));
    }

    #[test]
    fn a_default_on_self_is_an_error() {
        assert_eq!(
            errors(quote! { impl S { fn f(self = S) {} } }),
            ["`self` takes no default value"]
        );
    }

    #[test]
    fn a_required_parameter_after_a_defaulted_one_is_an_error() {
        let found = errors(quote! { fn f(a: u8 = 1, b: u8, c: u8 = 2, d: u8) {} });
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].starts_with("a parameter without a default cannot follow"));
    }

    #[test]
    fn a_default_without_a_body_is_an_error() {
        let found = errors(quote! {
            trait T { fn t(&self, a: u8 = 1); }
            extern "C" { fn c(a: u8 = 1); }
        });
        assert_eq!(found.len(), 2, "{found:?}");
        assert!(found[0].starts_with("a parameter default needs a fn body"));
    }

    #[test]
    fn an_unparsable_default_is_an_error() {
        let found = errors(quote! { fn f(a: u8 = struct) {} });
        assert_eq!(found.len(), 1, "{found:?}");
    }
}
