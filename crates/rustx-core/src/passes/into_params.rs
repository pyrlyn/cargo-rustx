//! `name: into T` parameters → `impl Into<T>` plus a conversion at the top of the body (problem 15).
//!
//! Two halves, because the sugar does not parse but the conversion needs the parsed fn:
//!
//! 1. Token pass ([`marker`]): in the parameter list of `fn name<..>(..)`, `x: into T` becomes
//!    `#[__rustx_into] x: T`, which `syn` parses.
//! 2. AST pass ([`convert`]): every fn signature takes its markers off, the parameter type
//!    becomes `impl ::core::convert::Into<T>` and the body starts with the conversion:
//!
//! ```text
//! fn new(name: into String) -> Self { Self { name } }
//! fn new(name: impl ::core::convert::Into<String>) -> Self {
//!     let name: String = ::core::convert::Into::into(name);
//!     Self { name }
//! }
//! ```
//!
//! A declaration without a body (a trait method, an `extern` fn) only gets the type: each
//! implementation converts in its own body, and implementing it with `into T` again matches.
//! Generated tokens take the span of `into`.
//!
//! # Detection
//!
//! `into` is sugar only right after the parameter's `:` and before a token that starts a type:
//! an identifier, `&`, `*`, `(..)` or `[..]`. So `x.into()`, a parameter named `into`, and the
//! types `into::T`, `into<T>` and `into!()` stay as written. A type that starts with `<` or `::`
//! needs parentheses after `into`: `x: into (<T as Tr>::A)`. A bare `x: into` is an error; a
//! type really named `into` is spelled `r#into`.
//!
//! # Limitations
//!
//! - Only a plain name (`x`, `mut x`, `ref x`) takes `into`; a pattern has no single value to
//!   convert, and `self` cannot change its type.
//! - In an `async fn` the conversion runs when the future is first polled, so the `impl Into`
//!   value is held by the future until then.
//! - Signatures inside macro bodies are not lowered: the marker stays and rustc reports the
//!   unknown attribute `__rustx_into` at `into`.

mod convert;
mod marker;

use proc_macro2::TokenStream;

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    marker::rewrite(tokens)
}

pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    convert::lower(file)
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

    fn error(sugar: TokenStream) -> String {
        desugar(sugar)
            .err()
            .map(|err| err.to_string())
            .unwrap_or_default()
    }

    #[test]
    fn into_parameter_becomes_impl_into_and_a_conversion() {
        assert_eq!(
            desugar(quote! {
                fn new(name: into String, n: u8) -> S { S { name } }
            })
            .ok(),
            Some(reprint(quote! {
                fn new(name: impl ::core::convert::Into<String>, n: u8) -> S {
                    let name: String = ::core::convert::Into::into(name);
                    S { name }
                }
            }))
        );
    }

    #[test]
    fn conversions_come_first_in_parameter_order() {
        assert_eq!(
            desugar(quote! {
                impl S { fn f(&mut self, a: into String, b: into Vec<u8>) { g(); } }
            })
            .ok(),
            Some(reprint(quote! {
                impl S {
                    fn f(
                        &mut self,
                        a: impl ::core::convert::Into<String>,
                        b: impl ::core::convert::Into<Vec<u8> >
                    ) {
                        let a: String = ::core::convert::Into::into(a);
                        let b: Vec<u8> = ::core::convert::Into::into(b);
                        g();
                    }
                }
            }))
        );
    }

    #[test]
    fn binding_mode_moves_to_the_let_and_attributes_are_copied() {
        assert_eq!(
            desugar(quote! {
                fn f(#[cfg(unix)] mut a: into String, ref b: into u64) {}
            })
            .ok(),
            Some(reprint(quote! {
                fn f(
                    #[cfg(unix)] a: impl ::core::convert::Into<String>,
                    b: impl ::core::convert::Into<u64>
                ) {
                    #[cfg(unix)] let mut a: String = ::core::convert::Into::into(a);
                    let ref b: u64 = ::core::convert::Into::into(b);
                }
            }))
        );
    }

    #[test]
    fn declarations_without_a_body_only_get_the_type() {
        assert_eq!(
            desugar(quote! {
                trait Named { fn rename(&mut self, name: into String); }
                extern "C" { fn ext(x: into u8); }
            })
            .ok(),
            Some(reprint(quote! {
                trait Named { fn rename(&mut self, name: impl ::core::convert::Into<String>); }
                extern "C" { fn ext(x: impl ::core::convert::Into<u8>); }
            }))
        );
    }

    #[test]
    fn nested_fns_and_trait_defaults_are_converted() {
        assert_eq!(
            desugar(quote! {
                trait T { fn d(x: into u32) { fn inner(y: into u8) {} } }
            })
            .ok(),
            Some(reprint(quote! {
                trait T {
                    fn d(x: impl ::core::convert::Into<u32>) {
                        let x: u32 = ::core::convert::Into::into(x);
                        fn inner(y: impl ::core::convert::Into<u8>) {
                            let y: u8 = ::core::convert::Into::into(y);
                        }
                    }
                }
            }))
        );
    }

    #[test]
    fn into_and_inline_bounds_work_in_one_signature() {
        let expanded = crate::try_expand(quote! {
            fn f(a: T: Clone, name: into String, b: into U: Copy) {}
        })
        .map(|file| file.into_token_stream().to_string());
        assert_eq!(
            expanded.ok(),
            Some(reprint(quote! {
                fn f<T, U>(
                    a: T,
                    name: impl ::core::convert::Into<String>,
                    b: impl ::core::convert::Into<U>
                ) where T: Clone, U: Copy {
                    let name: String = ::core::convert::Into::into(name);
                    let b: U = ::core::convert::Into::into(b);
                }
            }))
        );
    }

    #[test]
    fn valid_rust_passes_through_unchanged() {
        let valid = quote! {
            fn f(into: u8, x: impl Into<String>, p: into::T) -> String {
                let s: String = x.into();
                Into::into(s)
            }
            struct S { into: u8 }
        };
        assert_eq!(desugar(valid.clone()).ok(), Some(reprint(valid)));
    }

    #[test]
    fn into_on_self_is_an_error() {
        let err = error(quote! { impl S { fn f(self: into Box<Self>) {} } });
        assert!(err.starts_with("`self` cannot take `into`"), "{err}");
    }

    #[test]
    fn into_on_a_pattern_is_an_error() {
        let message = "`into` needs a plain parameter name such as `name`, not a pattern";
        assert_eq!(error(quote! { fn f((a, b): into (u8, u8)) {} }), message);
        assert_eq!(error(quote! { fn f(_: into u8) {} }), message);
        assert_eq!(error(quote! { fn f(x @ _: into u8) {} }), message);
    }

    #[test]
    fn errors_from_several_signatures_are_reported_together() {
        let err = desugar(quote! {
            fn f((a, b): into (u8, u8)) {}
            impl S { fn g(self: into Self, _: into u8) {} }
        })
        .err()
        .map(|err| err.into_iter().count());
        assert_eq!(err, Some(3));
    }
}
