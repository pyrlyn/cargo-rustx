//! Default arguments at call sites (problem 17): a call in the same input to a fn with parameter
//! defaults may leave defaulted arguments out.
//!
//! An AST pass that runs before `param_defaults::lower`, while every defaulted parameter still
//! carries its marker (see `param_defaults::default_of`). Arguments at defaulted positions
//! become `Some(..)`, omitted trailing ones `None`; the others stay as written:
//!
//! ```text
//! fn connect(host: &str, port: u16 = 5432, timeout_ms: u64 = 5_000) -> String { .. }
//! connect("db")        → connect("db", ::core::option::Option::None, ::core::option::Option::None)
//! connect("db", 6432)  → connect("db", ::core::option::Option::Some(6432), ::core::option::Option::None)
//! ```
//!
//! `Some(..)` takes the span of its argument, `None` the span of the call's `)`.
//!
//! # Resolution
//!
//! There are no types, so calls are matched by name, as `#[borrows]` matches types:
//!
//! - A path call (`connect(..)`, `server::connect(..)`, `Pool::new(..)`) resolves by its last
//!   segment to fns without a receiver. A bare name reaches free fns only; a qualified one free
//!   fns (the qualifier may be a module) and associated fns of an `impl` or trait whose type has
//!   the qualifier as its last segment. `Self::f(..)` means the enclosing `impl` or trait.
//! - A method call `x.m(..)` resolves to every method named `m` with a receiver.
//! - Only names with at least one defaulted candidate are sugar. Fns without defaults count as
//!   candidates too: when the candidates would rewrite a call differently, it is reported as
//!   ambiguous instead of being filled in for the wrong fn. Candidates of the same shape are fine.
//!
//! Calls are rewritten in fn bodies, in parameter defaults, and in macro bodies that parse as
//! comma-separated expressions or as `value; count` (`vec![..]`, `format!(..)`,
//! `assert_eq!(..)`); other macro bodies are left alone. A body without such calls keeps its
//! tokens exactly.
//!
//! # Errors
//!
//! Reported together, on the user's tokens: too many arguments (at the first one too many), a
//! missing argument without a default (at the call's `)`), and an ambiguous name (at the name).
//!
//! # Limitations
//!
//! - Matching by name ignores modules and types: a defaulted method `len` also rewrites
//!   `vec.len()`, and a free fn `new` with defaults rewrites `Vec::new()`. Rename such fns.
//! - `<T as Trait>::f(..)` and method calls through the path form (`Type::method(&x, ..)`) are not
//!   resolved; they need every argument, as `Option`s.
//! - Calls outside the input see the real `Option<T>` signature.

mod collect;
mod plan;
mod walk;

pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    walk::lower(file)
}

#[cfg(test)]
mod tests {
    use proc_macro2::TokenStream;
    use quote::{ToTokens, quote};
    use syn::Item;

    /// The body of the last `fn demo` after the whole pipeline, or the errors.
    fn demo(sugar: TokenStream) -> String {
        match crate::try_expand(sugar) {
            Ok(file) => file
                .items
                .iter()
                .rev()
                .find_map(|item| match item {
                    Item::Fn(item) if item.sig.ident == "demo" => {
                        Some(item.block.to_token_stream().to_string())
                    }
                    _ => None,
                })
                .unwrap_or_default(),
            Err(err) => err
                .into_iter()
                .map(|err| err.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    fn block(plain: TokenStream) -> String {
        syn::parse2::<syn::Block>(plain)
            .map(|block| block.to_token_stream().to_string())
            .unwrap_or_default()
    }

    fn connect() -> TokenStream {
        quote! {
            pub fn connect(host: &str, port: u16 = 5432, timeout_ms: u64 = 5_000) -> String {
                format!("{host}:{port} ({timeout_ms} ms)")
            }
        }
    }

    #[test]
    fn omitted_defaults_become_none_and_given_ones_some() {
        let connect = connect();
        assert_eq!(
            demo(quote! {
                #connect
                fn demo() {
                    connect("db");
                    connect("db", 6432);
                    connect("db", 6432, 100);
                }
            }),
            block(quote! {{
                connect("db", ::core::option::Option::None, ::core::option::Option::None);
                connect("db", ::core::option::Option::Some(6432), ::core::option::Option::None);
                connect(
                    "db",
                    ::core::option::Option::Some(6432),
                    ::core::option::Option::Some(100)
                );
            }})
        );
    }

    #[test]
    fn calls_in_expression_macros_are_rewritten() {
        let connect = connect();
        assert_eq!(
            demo(quote! {
                #connect
                fn demo() {
                    vec![connect("db"), format!("{}", connect("x", 1))];
                    vec![connect("db"); 2];
                }
            }),
            block(quote! {{
                vec![
                    connect("db", ::core::option::Option::None, ::core::option::Option::None),
                    format!(
                        "{}",
                        connect("x", ::core::option::Option::Some(1), ::core::option::Option::None)
                    )
                ];
                vec![
                    connect("db", ::core::option::Option::None, ::core::option::Option::None);
                    2
                ];
            }})
        );
    }

    #[test]
    fn other_macro_bodies_and_calls_without_defaults_are_left_alone() {
        let connect = connect();
        let body = quote! {{
            m!(connect("db") => x);
            vec![other("db"), connect];
            other("db");
            ::std::thread::spawn(|| {});
        }};
        assert_eq!(
            demo(quote! { #connect fn other(a: &str) {} fn demo() #body }),
            block(body)
        );
    }

    #[test]
    fn methods_associated_fns_and_self_paths_resolve() {
        assert_eq!(
            demo(quote! {
                struct Pool;
                impl Pool {
                    fn new(size: usize = 4) -> Self { Pool }
                    fn spawn(&self, n: usize = 1) {}
                    fn make() -> Self { Self::new() }
                }
                fn demo() {
                    Pool::new().spawn();
                    crate::Pool::new(8).spawn(2);
                    Vec::<u8>::new();
                }
            }),
            block(quote! {{
                Pool::new(::core::option::Option::None).spawn(::core::option::Option::None);
                crate::Pool::new(::core::option::Option::Some(8))
                    .spawn(::core::option::Option::Some(2));
                Vec::<u8>::new();
            }})
        );
    }

    #[test]
    fn calls_inside_parameter_defaults_and_nested_calls_are_rewritten() {
        let expanded = crate::try_expand(quote! {
            fn base(scale: u64 = 1) -> u64 { scale }
            fn wait(ms: u64 = base() * 10) -> u64 { base(base(ms)) }
        })
        .map(|file| file.into_token_stream().to_string());
        let expected = syn::parse2::<syn::File>(quote! {
            fn base(scale: ::core::option::Option<u64>) -> u64 {
                let scale: u64 = scale.unwrap_or(1);
                scale
            }
            fn wait(ms: ::core::option::Option<u64>) -> u64 {
                let ms: u64 = ms.unwrap_or_else(|| base(::core::option::Option::None) * 10);
                base(::core::option::Option::Some(base(::core::option::Option::Some(ms))))
            }
        })
        .map(|file| file.into_token_stream().to_string());
        assert_eq!(expanded.ok(), expected.ok());
    }

    #[test]
    fn candidates_of_the_same_shape_are_fine_and_different_ones_ambiguous() {
        let input = quote! {
            mod a { pub fn open(path: &str, mode: u8 = 0) {} }
            mod b { pub fn open(path: &str, flags: u8 = 1) {} }
            mod c { pub fn close(fd: u8) {} }
            fn close(fd: u8, force: bool = false) {}
        };
        assert_eq!(
            demo(quote! { #input fn demo() { a::open("x"); } }),
            block(quote! {{ a::open("x", ::core::option::Option::None); }})
        );
        assert_eq!(
            demo(quote! { #input fn demo() { c::close(1); } }),
            "`close` matches several fns in this input that read this call differently; \
             calls are matched by name only, so rename one of them"
        );
    }

    #[test]
    fn too_many_and_missing_arguments_are_reported_together() {
        let connect = connect();
        assert_eq!(
            demo(quote! { #connect fn demo() { connect("db", 1, 2, 3); connect(); } }),
            "too many arguments: `connect` takes at most 3\n\
             missing argument `host` of `connect`; only parameters with a default can be left out"
        );
    }

    #[test]
    fn valid_rust_passes_through_unchanged() {
        let valid = quote! {
            fn connect(host: &str, port: Option<u16>) -> String { format!("{host}{port:?}") }
            fn demo() { connect("db", None); vec![connect("db", Some(1))]; }
        };
        let expanded = crate::try_expand(valid.clone()).map(|file| file.to_token_stream());
        assert_eq!(
            expanded.map(|tokens| tokens.to_string()).ok(),
            Some(valid.to_string())
        );
    }
}
