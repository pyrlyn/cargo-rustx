//! The items of one `#[newtype]` struct: the inherent `impl` with `new`, then one impl per flag
//! in the order written.

use proc_macro2::{Ident, Span, TokenStream};
use quote::{ToTokens, quote_spanned};
use syn::{GenericArgument, Item, PathArguments, Type};

use super::{Flag, Kind, Newtype};
use crate::passes::impl_header::ImplHeader;
use crate::tokens::syntax::{introduced_span, is_string, std_type};

pub(super) fn generate(newtype: &Newtype, flags: &[Flag]) -> syn::Result<Vec<Item>> {
    let mut items = vec![constructor(newtype)];
    items.extend(flags.iter().map(|flag| flag_impl(newtype, flag)));
    items.into_iter().map(syn::parse2).collect()
}

/// `pub fn new`, fallible and checked when the struct has a validator.
fn constructor(newtype: &Newtype) -> TokenStream {
    let span = newtype.span;
    let (ident, inner) = (&newtype.item.ident, newtype.inner);
    let (impl_generics, type_generics, where_clause) = newtype.item.generics.split_for_impl();
    let value = Ident::new("__rustx_inner", introduced_span(span));
    let new = match newtype.validate {
        None => quote_spanned! {span=>
            pub fn new(#value: #inner) -> Self { Self(#value) }
        },
        Some((validator, error)) => quote_spanned! {span=>
            pub fn new(#value: #inner) -> ::core::result::Result<Self, #error> {
                #validator(&#value)?;
                ::core::result::Result::Ok(Self(#value))
            }
        },
    };
    quote_spanned! {span=>
        #[allow(dead_code)]
        impl #impl_generics #ident #type_generics #where_clause { #new }
    }
}

fn flag_impl(newtype: &Newtype, flag: &Flag) -> TokenStream {
    let span = flag.name.span();
    let inner = newtype.inner;
    let header = ImplHeader {
        ident: &newtype.item.ident,
        generics: &newtype.item.generics,
    };
    let (tr, body) = match flag.kind {
        Kind::Deref => {
            let target = target(inner, span);
            let tr = quote_spanned!(span=> ::core::ops::Deref);
            let body = quote_spanned! {span=>
                type Target = #target;
                fn deref(&self) -> &#target { &self.0 }
            };
            (tr, body)
        }
        Kind::AsRef => {
            let target = target(inner, span);
            let tr = quote_spanned!(span=> ::core::convert::AsRef<#target>);
            let body = quote_spanned!(span=> fn as_ref(&self) -> &#target { &self.0 });
            (tr, body)
        }
        Kind::Display => (quote_spanned!(span=> ::core::fmt::Display), display(flag)),
        Kind::FromStr => (
            quote_spanned!(span=> ::core::str::FromStr),
            from_str(newtype, flag),
        ),
        Kind::Into => return into(newtype, flag),
        Kind::Operator(name) => {
            let tr = Ident::new(name, span);
            let method = &flag.name;
            let other = Ident::new("__rustx_other", introduced_span(span));
            let body = quote_spanned! {span=>
                type Output = Self;
                fn #method(self, #other: Self) -> Self {
                    Self(::core::ops::#tr::#method(self.0, #other.0))
                }
            };
            (quote_spanned!(span=> ::core::ops::#tr), body)
        }
    };
    header.implement(span, &tr, &body)
}

/// What `deref` and `as_ref` lend: `str` for a `String`, `[T]` for a `Vec<T>`, else the inner
/// type itself, as in problem 20.
fn target(inner: &Type, span: Span) -> TokenStream {
    if is_string(inner) {
        quote_spanned!(span=> str)
    } else if let Some(element) = vec_element(inner) {
        quote_spanned!(span=> [#element])
    } else {
        inner.to_token_stream()
    }
}

/// `T` of `Vec<T>`, `std::vec::Vec<T>` or `alloc::vec::Vec<T>`.
fn vec_element(ty: &Type) -> Option<&Type> {
    let PathArguments::AngleBracketed(args) = std_type(ty, "vec", "Vec")? else {
        return None;
    };
    match (args.args.first(), args.args.len()) {
        (Some(GenericArgument::Type(element)), 1) => Some(element),
        _ => None,
    }
}

/// `fmt` forwarding to the inner value, or writing the `display = ".."` string.
fn display(flag: &Flag) -> TokenStream {
    let span = flag.name.span();
    let f = Ident::new("__rustx_f", introduced_span(span));
    let write = match &flag.format {
        None => quote_spanned!(span=> ::core::fmt::Display::fmt(&self.0, #f)),
        Some((format, uses_inner)) => {
            let inner = uses_inner.then(|| quote_spanned!(format.span()=> , &self.0));
            quote_spanned!(format.span()=> ::core::write!(#f, #format #inner))
        }
    };
    quote_spanned! {span=>
        fn fmt(&self, #f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { #write }
    }
}

/// `from_str` through the inner type's `FromStr`, or through `new` when there is a validator.
fn from_str(newtype: &Newtype, flag: &Flag) -> TokenStream {
    let span = flag.name.span();
    let inner = newtype.inner;
    let s = Ident::new("__rustx_s", introduced_span(span));
    let parse = quote_spanned!(span=> <#inner as ::core::str::FromStr>::from_str(#s));
    let (error, body) = match newtype.validate {
        None => (
            quote_spanned!(span=> <#inner as ::core::str::FromStr>::Err),
            quote_spanned!(span=> #parse.map(Self)),
        ),
        // A `String` cannot fail to parse, and its `Infallible` error converts into no `E`.
        Some((_, error)) if is_string(inner) => (
            error.to_token_stream(),
            quote_spanned!(span=> Self::new(::core::convert::From::from(#s))),
        ),
        Some((_, error)) => (
            error.to_token_stream(),
            quote_spanned!(span=> Self::new(#parse?)),
        ),
    };
    quote_spanned! {span=>
        type Err = #error;
        fn from_str(#s: &str) -> ::core::result::Result<Self, Self::Err> { #body }
    }
}

/// `From<Newtype> for Inner`: the one impl that is for the inner type, not the newtype.
fn into(newtype: &Newtype, flag: &Flag) -> TokenStream {
    let span = flag.name.span();
    let (ident, inner) = (&newtype.item.ident, newtype.inner);
    let (impl_generics, type_generics, where_clause) = newtype.item.generics.split_for_impl();
    let value = Ident::new("__rustx_value", introduced_span(span));
    quote_spanned! {span=>
        impl #impl_generics ::core::convert::From<#ident #type_generics> for #inner #where_clause {
            fn from(#value: #ident #type_generics) -> Self { #value.0 }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{lowered, printed};
    use super::*;
    use quote::quote;

    #[test]
    fn the_problem_example_generates_new_and_one_impl_per_flag() {
        assert_eq!(
            lowered(quote! {
                #[newtype(deref, as_ref, display, from_str, into, validate = check_email)]
                pub struct Email(String);
                fn check_email(s: &String) -> Result<(), String> { Ok(()) }
                #[newtype(add, display = "{} m")]
                pub struct Meters(pub f64);
            })
            .unwrap(),
            printed(quote! {
                pub struct Email(String);
                #[allow(dead_code)]
                impl Email {
                    pub fn new(__rustx_inner: String) -> ::core::result::Result<Self, String> {
                        check_email(&__rustx_inner)?;
                        ::core::result::Result::Ok(Self(__rustx_inner))
                    }
                }
                impl ::core::ops::Deref for Email {
                    type Target = str;
                    fn deref(&self) -> &str { &self.0 }
                }
                impl ::core::convert::AsRef<str> for Email {
                    fn as_ref(&self) -> &str { &self.0 }
                }
                impl ::core::fmt::Display for Email {
                    fn fmt(&self, __rustx_f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        ::core::fmt::Display::fmt(&self.0, __rustx_f)
                    }
                }
                impl ::core::str::FromStr for Email {
                    type Err = String;
                    fn from_str(__rustx_s: &str) -> ::core::result::Result<Self, Self::Err> {
                        Self::new(::core::convert::From::from(__rustx_s))
                    }
                }
                impl ::core::convert::From<Email> for String {
                    fn from(__rustx_value: Email) -> Self { __rustx_value.0 }
                }
                fn check_email(s: &String) -> Result<(), String> { Ok(()) }
                pub struct Meters(pub f64);
                #[allow(dead_code)]
                impl Meters {
                    pub fn new(__rustx_inner: f64) -> Self { Self(__rustx_inner) }
                }
                impl ::core::ops::Add for Meters {
                    type Output = Self;
                    fn add(self, __rustx_other: Self) -> Self {
                        Self(::core::ops::Add::add(self.0, __rustx_other.0))
                    }
                }
                impl ::core::fmt::Display for Meters {
                    fn fmt(&self, __rustx_f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        ::core::write!(__rustx_f, "{0} m", &self.0)
                    }
                }
            })
        );
    }

    #[test]
    fn generics_are_copied_and_vec_derefs_to_a_slice() {
        assert_eq!(
            lowered(quote! {
                #[newtype(deref, into)]
                struct List<T: Clone, const N: usize = 1>(Vec<[T; N]>) where T: Default;
            })
            .unwrap(),
            printed(quote! {
                struct List<T: Clone, const N: usize = 1>(Vec<[T; N]>) where T: Default;
                #[allow(dead_code)]
                impl<T: Clone, const N: usize> List<T, N> where T: Default {
                    pub fn new(__rustx_inner: Vec<[T; N]>) -> Self { Self(__rustx_inner) }
                }
                impl<T: Clone, const N: usize> ::core::ops::Deref for List<T, N> where T: Default {
                    type Target = [[T; N]];
                    fn deref(&self) -> &[[T; N]] { &self.0 }
                }
                impl<T: Clone, const N: usize> ::core::convert::From<List<T, N> > for Vec<[T; N]>
                where T: Default {
                    fn from(__rustx_value: List<T, N>) -> Self { __rustx_value.0 }
                }
            })
        );
    }

    #[test]
    fn from_str_parses_the_inner_type_with_or_without_a_validator() {
        let unchecked = lowered(quote! { #[newtype(from_str)] struct Port(u16); }).unwrap();
        let expected = quote! {
            type Err = <u16 as ::core::str::FromStr>::Err;
            fn from_str(__rustx_s: &str) -> ::core::result::Result<Self, Self::Err> {
                <u16 as ::core::str::FromStr>::from_str(__rustx_s).map(Self)
            }
        };
        assert!(unchecked.contains(&expected.to_string()), "{unchecked}");
        let checked = lowered(quote! {
            fn nonzero(p: &u16) -> core::result::Result<(), Bad> { Ok(()) }
            #[newtype(from_str, validate = nonzero)] struct Port(u16);
        })
        .unwrap();
        let expected = quote! {
            type Err = Bad;
            fn from_str(__rustx_s: &str) -> ::core::result::Result<Self, Self::Err> {
                Self::new(<u16 as ::core::str::FromStr>::from_str(__rustx_s)?)
            }
        };
        assert!(checked.contains(&expected.to_string()), "{checked}");
    }

    #[test]
    fn other_inner_types_are_their_own_target() {
        let ty = |source: &str| syn::parse_str::<Type>(source).unwrap();
        let span = proc_macro2::Span::call_site();
        let target = |source: &str| target(&ty(source), span).to_string();
        assert_eq!(target("std::string::String"), "str");
        assert_eq!(target("alloc::vec::Vec<u8>"), "[u8]");
        for own in [
            "Box<str>",
            "Vec<u8, A>",
            "my::Vec<u8>",
            "Vec",
            "u64",
            "VecDeque<u8>",
        ] {
            assert_eq!(target(own), ty(own).to_token_stream().to_string(), "{own}");
        }
    }
}
