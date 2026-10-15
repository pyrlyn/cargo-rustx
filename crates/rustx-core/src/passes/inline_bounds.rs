//! Inline bounds `x: T: Clone + Debug` hoisted into generics and `where` (problem 03).
//!
//! Two halves, because the sugar does not parse but the hoisting needs the parsed fn header:
//!
//! 1. Token pass ([`marker`]): in the parameter list of `fn name<..>(..)`, a parameter with a
//!    second top-level `:` (`x: T: B`) becomes `#[__rustx_bound(B)] x: T`, which `syn` parses.
//!    No valid parameter has a second top-level `:`, so valid Rust is never touched.
//! 2. AST pass ([`hoist`]): every fn signature (free fns, methods, trait and foreign fns) takes
//!    its markers off, declares the named generics and appends the bounds to `where`:
//!
//! ```text
//! fn run(x: T: Clone + Debug + Send) -> Result<T, E: Error>
//! fn run<T, E>(x: T) -> Result<T, E> where T: Clone + Debug + Send, E: Error
//! ```
//!
//! The return-type form `Result<T, E: Error>` already parses: it is an associated-type bound,
//! which Rust allows only on traits (`impl Iterator<Item: Clone>`). It is therefore sugar only
//! in a type path of a signature; bounds on trait paths (`impl`, `dyn`, `where`) stay as written.
//!
//! # New generic or existing name
//!
//! Types are invisible to a macro, so a bounded name is judged by these rules, in order:
//!
//! 1. A generic already in scope (the fn's own, the enclosing `impl` or `trait`, or `Self`):
//!    only the bound is added.
//! 2. A type the input declares or imports (`struct`, `enum`, `union`, `type`, `trait`, `use`),
//!    a primitive, or a prelude type (`Box`, `Option`, `Result`, `String`, `Vec`): an error,
//!    because a bound on a concrete type belongs in `where`.
//! 3. Anything else: a new generic parameter, declared once, in order of first appearance
//!    (parameters left to right, then the return type).
//!
//! The bounded type must be a bare name (`T`, not `Vec<T>` or `&T`). Bounds on one name from
//! several places are merged into one predicate.
//!
//! # Struct bounds
//!
//! A struct, enum or union of the input with bounded generics (`struct Cache<K: Hash, V>`) lends
//! its bounds to an `impl` header or signature that names it with undeclared parameters spelled
//! as in its declaration: `impl Cache<K, V>` becomes `impl<K, V> Cache<K, V> where K: Hash`,
//! and so does `fn warm_up(c: &mut Cache<K, V>)`. A name in another position, or one that rule 1
//! or 2 above covers, is left alone.
//!
//! # Limitations
//!
//! - A type brought in by a glob import (`use m::*`) is unknown, so `x: T: B` with such a `T`
//!   declares a generic that shadows it, and `Cache<K>` with a glob-imported `K` inherits.
//! - Signatures inside macro bodies are not lowered: the marker stays and rustc reports the
//!   unknown attribute `__rustx_bound` at the `:`.
//! - Closure parameters take no inline bounds; closures cannot declare generics.

mod hoist;
mod marker;

use proc_macro2::TokenStream;

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    marker::rewrite(tokens)
}

pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    hoist::lower(file)
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
    fn parameter_and_return_bounds_become_generics_and_where() {
        assert_eq!(
            desugar(quote! {
                fn run(x: T: Clone + Debug + Send) -> Result<T, E: Error> { Ok(x) }
            })
            .ok(),
            Some(reprint(quote! {
                fn run<T, E>(x: T) -> Result<T, E> where T: Clone + Debug + Send, E: Error { Ok(x) }
            }))
        );
    }

    #[test]
    fn existing_generics_and_where_are_kept() {
        assert_eq!(
            desugar(quote! {
                fn f<'a, U: Copy>(u: &'a U, x: T: Clone) where U: Debug {}
            })
            .ok(),
            Some(reprint(quote! {
                fn f<'a, U: Copy, T>(u: &'a U, x: T) where U: Debug, T: Clone {}
            }))
        );
    }

    #[test]
    fn a_generic_already_in_scope_only_gets_the_bound() {
        assert_eq!(
            desugar(quote! {
                impl<T> S<T> { fn f(&self, x: T: Clone, y: Self: Copy) {} }
                trait Tr<U> { fn g(u: U: Debug); }
                fn h<V>(v: V: Send) {}
            })
            .ok(),
            Some(reprint(quote! {
                impl<T> S<T> { fn f(&self, x: T, y: Self) where T: Clone, Self: Copy {} }
                trait Tr<U> { fn g(u: U) where U: Debug; }
                fn h<V>(v: V) where V: Send {}
            }))
        );
    }

    #[test]
    fn a_name_bounded_twice_is_declared_once_with_merged_bounds() {
        assert_eq!(
            desugar(quote! { fn f(a: T: Clone, b: T: Debug + Clone) -> Vec<T: Send> {} }).ok(),
            Some(reprint(quote! {
                fn f<T>(a: T, b: T) -> Vec<T> where T: Clone + Debug + Send {}
            }))
        );
    }

    #[test]
    fn nested_fns_do_not_see_the_generics_of_the_outer_item() {
        assert_eq!(
            desugar(quote! {
                impl<T> S<T> { fn f() { fn g(x: T: Clone) {} } }
            })
            .ok(),
            Some(reprint(quote! {
                impl<T> S<T> { fn f() { fn g<T>(x: T) where T: Clone {} } }
            }))
        );
    }

    #[test]
    fn impl_and_signatures_inherit_struct_bounds() {
        assert_eq!(
            desugar(quote! {
                struct Cache<K: Hash, V> where V: Clone { m: HashMap<K, V> }
                impl Cache<K, V> { fn get(&self, k: K) -> Option<V> { None } }
                fn warm(c: &mut Cache<K, V>, items: I: IntoIterator<Item = (K, V)>) {}
            })
            .ok(),
            Some(reprint(quote! {
                struct Cache<K: Hash, V> where V: Clone { m: HashMap<K, V> }
                impl<K, V> Cache<K, V> where K: Hash, V: Clone {
                    fn get(&self, k: K) -> Option<V> { None }
                }
                fn warm<K, V, I>(c: &mut Cache<K, V>, items: I)
                where K: Hash, V: Clone, I: IntoIterator<Item = (K, V)> {}
            }))
        );
    }

    #[test]
    fn struct_bounds_follow_the_arguments_they_name() {
        assert_eq!(
            desugar(quote! {
                struct Pair<A, B: From<A>> { a: A, b: B }
                fn f(p: Pair<u8, B>) {}
            })
            .ok(),
            Some(reprint(quote! {
                struct Pair<A, B: From<A>> { a: A, b: B }
                fn f<B>(p: Pair<u8, B>) where B: From<u8> {}
            }))
        );
    }

    #[test]
    fn valid_rust_passes_through_unchanged() {
        let valid = quote! {
            struct Cache<K: Hash, V> { a: B, m: HashMap<K, V> }
            impl<K: Hash, V> Cache<K, V> { fn f(&self, k: K) {} }
            struct K;
            fn g(c: Cache<K, u8>, x: T, i: impl Iterator<Item: Clone>, p: <T as Tr>::A) {}
            fn h<I: Iterator<Item: Clone>>(i: I) -> Box<dyn Tr<A: Send>> where I: Tr<B: Copy> {
                let c = |x: u8| x;
                S { a: b }
            }
        };
        assert_eq!(desugar(valid.clone()).ok(), Some(reprint(valid)));
    }

    #[test]
    fn a_bound_on_a_type_that_is_not_a_bare_name_is_an_error() {
        let message = "an inline bound needs a bare generic name such as `T`";
        assert!(error(quote! { fn f(x: Vec<u8>: Clone) {} }).starts_with(message));
        assert!(error(quote! { fn f() -> Result<u8, E<u8>: Debug> {} }).starts_with(message));
    }

    #[test]
    fn a_bound_on_a_known_type_is_an_error() {
        let message = "`Meters` names a type here, not a new generic";
        assert!(error(quote! { struct Meters; fn f(m: Meters: Clone) {} }).starts_with(message));
        assert!(error(quote! { fn f(s: String: Clone) {} }).starts_with("`String` names a type"));
        let imported = error(quote! { use std::fmt::{Debug as D}; fn f(d: D: Clone) {} });
        assert!(imported.starts_with("`D` names a type"), "{imported}");
    }

    #[test]
    fn a_bound_on_self_is_an_error() {
        let err = error(quote! { impl S { fn f(self: Self: Clone) {} } });
        assert!(err.starts_with("`self` takes no inline bound"), "{err}");
    }

    #[test]
    fn errors_from_several_parameters_are_reported_together() {
        let err = desugar(quote! { fn f(a: Vec<u8>: Clone, b: String: Copy) {} })
            .err()
            .map(|err| err.into_iter().count());
        assert_eq!(err, Some(2));
    }
}
