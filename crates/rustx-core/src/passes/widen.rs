//! Implicit lossless number conversions under `#[widen]` (problem 13, `13-as-into.md`).
//!
//! Explicit opt-in (ARCHITECTURE.md, section g): `#[widen]` on a fn, an `impl` or a module, or
//! `#![widen]` at the start of the input, marks the fn bodies inside it. There, a value goes into
//! a place whose type the input writes (see `typed_positions`) and whose own type the input
//! writes too gets converted when std has a lossless `From` between the two numbers:
//!
//! ```text
//! let v: u32 = latency_us;  // latency_us: u16
//! → let v: u32 = <::core::primitive::u32 as ::core::convert::From<::core::primitive::u16>>::from(latency_us);
//! ```
//!
//! The conversion names both types instead of calling `.into()`: a method call could resolve to
//! an inherent or other `into`, `x += y.into()` cannot infer its target (`u64: AddAssign<&u64>`
//! fits too), and a source type guessed wrong, say through a binding made inside a macro, then
//! fails to compile instead of picking another conversion. Literals, lossy pairs (`u64` to `f64`,
//! `usize` to anything), shifts and everything whose types are not both written stay as written.

use std::mem;

use proc_macro2::Ident;
use quote::ToTokens;
use syn::spanned::Spanned;
use syn::visit_mut::{self, VisitMut};
use syn::{
    Attribute, Block, Expr, ImplItemFn, ItemFn, ItemImpl, ItemMod, Meta, Signature, TraitItemFn,
    Type,
};

use super::call_defaults::collect::type_name;
use super::typed_positions::{Convert, Declared, walk_fn};
use crate::tokens::syntax::{bare_name, combine};

const ATTRIBUTE: &str = "widen";

/// Each source number and the numbers std converts it into with `From`, so without loss.
/// `usize` and `isize` take only what fits on every platform; nothing converts from them.
const WIDENINGS: &[(&str, &str)] = &[
    (
        "u8",
        "u16 u32 u64 u128 usize i16 i32 i64 i128 isize f32 f64",
    ),
    ("u16", "u32 u64 u128 usize i32 i64 i128 f32 f64"),
    ("u32", "u64 u128 i64 i128 f64"),
    ("u64", "u128 i128"),
    ("i8", "i16 i32 i64 i128 isize f32 f64"),
    ("i16", "i32 i64 i128 isize f32 f64"),
    ("i32", "i64 i128 f64"),
    ("i64", "i128"),
    ("f32", "f64"),
];

pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    let declared = Declared::collect(file);
    let mut marks = Marks {
        declared: &declared,
        marked: false,
        owner: None,
        errors: None,
    };
    marks.marked = marks.take(&mut file.attrs);
    marks.visit_file_mut(file);
    marks.errors.map_or(Ok(()), Err)
}

/// The walk over the file that finds what `#[widen]` marks and takes the attribute off.
struct Marks<'a> {
    declared: &'a Declared,
    marked: bool,
    /// The type of the enclosing `impl`, for `Self` and `self`.
    owner: Option<String>,
    errors: Option<syn::Error>,
}

impl Marks<'_> {
    fn error(&mut self, tokens: &impl ToTokens, message: &str) {
        combine(&mut self.errors, syn::Error::new_spanned(tokens, message));
    }

    /// Takes `#[widen]` off `attrs`; whether it was there.
    fn take(&mut self, attrs: &mut Vec<Attribute>) -> bool {
        let mut found = false;
        attrs.retain(|attr| {
            if !attr.path().is_ident(ATTRIBUTE) {
                return true;
            }
            if !matches!(attr.meta, Meta::Path(_)) {
                self.error(&attr.meta, "`#[widen]` takes no arguments");
            }
            if found {
                self.error(attr, "`#[widen]` is given twice");
            }
            found = true;
            false
        });
        found
    }

    fn within(&mut self, marked: bool, owner: Option<String>, visit: impl FnOnce(&mut Self)) {
        let outer_marked = self.marked;
        self.marked |= marked;
        let outer_owner = mem::replace(&mut self.owner, owner);
        visit(self);
        self.marked = outer_marked;
        self.owner = outer_owner;
    }

    fn body(&self, sig: &mut Signature, block: &mut Block) {
        if self.marked {
            walk_fn(self.declared, self.owner.as_deref(), sig, block, &mut Widen);
        }
    }
}

impl VisitMut for Marks<'_> {
    fn visit_item_mod_mut(&mut self, item: &mut ItemMod) {
        let marked = self.take(&mut item.attrs);
        self.within(marked, None, |marks| {
            visit_mut::visit_item_mod_mut(marks, item);
        });
    }

    fn visit_item_impl_mut(&mut self, item: &mut ItemImpl) {
        let marked = self.take(&mut item.attrs);
        self.within(marked, Some(type_name(&item.self_ty)), |marks| {
            visit_mut::visit_item_impl_mut(marks, item);
        });
    }

    fn visit_item_fn_mut(&mut self, item: &mut ItemFn) {
        let marked = self.take(&mut item.attrs);
        self.within(marked, None, |marks| {
            marks.body(&mut item.sig, &mut item.block);
            visit_mut::visit_item_fn_mut(marks, item);
        });
    }

    fn visit_impl_item_fn_mut(&mut self, item: &mut ImplItemFn) {
        let marked = self.take(&mut item.attrs);
        self.within(marked, self.owner.clone(), |marks| {
            marks.body(&mut item.sig, &mut item.block);
            visit_mut::visit_impl_item_fn_mut(marks, item);
        });
    }

    fn visit_trait_item_fn_mut(&mut self, item: &mut TraitItemFn) {
        let marked = self.take(&mut item.attrs);
        self.within(marked, None, |marks| {
            if let Some(block) = &mut item.default {
                marks.body(&mut item.sig, block);
            }
            visit_mut::visit_trait_item_fn_mut(marks, item);
        });
    }

    /// Reached only by a `#[widen]` that no fn, `impl` or module took off.
    fn visit_attribute_mut(&mut self, attr: &mut Attribute) {
        if attr.path().is_ident(ATTRIBUTE) {
            self.error(
                attr,
                "`#[widen]` goes on a fn, an `impl` or a module, or as `#![widen]` at the start \
                 of the input",
            );
        }
    }
}

/// The lossless number conversions of problem 13.
struct Widen;

impl Convert for Widen {
    fn convert(&mut self, target: &Type, value: &mut Expr, source: Option<&Type>) {
        let (Some(to), Some(from)) = (number(target), source.and_then(number)) else {
            return;
        };
        if !WIDENINGS
            .iter()
            .any(|&(source, targets)| source == from && targets.split(' ').any(|t| t == to))
        {
            return;
        }
        let span = value.span();
        let (to, from) = (Ident::new(to, span), Ident::new(from, span));
        let inner = mem::replace(value, Expr::PLACEHOLDER);
        *value = syn::parse_quote_spanned! {span=>
            <::core::primitive::#to as ::core::convert::From<::core::primitive::#from>>::from(#inner)
        };
    }
}

/// The name of a number type written bare, as std spells it.
fn number(ty: &Type) -> Option<&'static str> {
    let name = bare_name(ty)?;
    WIDENINGS
        .iter()
        .flat_map(|&(source, targets)| std::iter::once(source).chain(targets.split(' ')))
        .find(|number| name == number)
}

#[cfg(test)]
mod tests {
    use proc_macro2::TokenStream;
    use quote::{ToTokens, format_ident, quote};

    fn expanded(sugar: TokenStream) -> String {
        match crate::try_expand(sugar) {
            Ok(file) => file.into_token_stream().to_string(),
            Err(err) => err
                .into_iter()
                .map(|err| err.to_string())
                .collect::<Vec<_>>()
                .join("\n"),
        }
    }

    fn plain(rust: TokenStream) -> String {
        syn::parse2::<syn::File>(rust)
            .unwrap()
            .into_token_stream()
            .to_string()
    }

    /// The conversion the pass writes, applied to what follows it in parentheses.
    fn from(to: &str, from: &str) -> TokenStream {
        let (to, from) = (format_ident!("{to}"), format_ident!("{from}"));
        quote!(<::core::primitive::#to as ::core::convert::From<::core::primitive::#from>>::from)
    }

    #[test]
    fn nothing_changes_without_the_attribute() {
        let input = quote! {
            struct S { total: u64 }
            fn f(n: u32, s: &mut S) { let x: u64 = n; s.total += n; g(n); }
            fn g(x: u64) {}
        };
        assert_eq!(expanded(input.clone()), plain(input));
    }

    #[test]
    fn lossless_pairs_between_written_types_are_converted_in_every_position() {
        let (u64_u32, i64_i16, f64_f32) =
            (from("u64", "u32"), from("i64", "i16"), from("f64", "f32"));
        assert_eq!(
            expanded(quote! {
                struct S { total: u64, peak: i64 }
                struct T(f64);
                impl S { fn new(total: u64) -> Self { S { total, peak: 0 } } }
                fn g(x: u64, y: f64) {}
                #[widen]
                fn f(n: u32, level: i16, r: f32, s: &mut S, t: &mut T) {
                    let x: u64 = n;
                    let mut y: u64 = 0;
                    y = n;
                    y += (n);
                    s.total -= n;
                    (*s).peak = level;
                    t.0 = r;
                    g(n, r);
                    S::new(n);
                }
            }),
            plain(quote! {
                struct S { total: u64, peak: i64 }
                struct T(f64);
                impl S { fn new(total: u64) -> Self { S { total, peak: 0 } } }
                fn g(x: u64, y: f64) {}
                fn f(n: u32, level: i16, r: f32, s: &mut S, t: &mut T) {
                    let x: u64 = #u64_u32(n);
                    let mut y: u64 = 0;
                    y = #u64_u32(n);
                    y += #u64_u32((n));
                    s.total -= #u64_u32(n);
                    (*s).peak = #i64_i16(level);
                    t.0 = #f64_f32(r);
                    g(#u64_u32(n), #f64_f32(r));
                    S::new(#u64_u32(n));
                }
            })
        );
    }

    #[test]
    fn lossy_pairs_literals_shifts_unknown_types_and_other_positions_stay_as_written() {
        let body = quote! {{
            let a: f64 = big;
            let b: u32 = big;
            let c: isize = small;
            let d: u64 = size;
            let e: f32 = mid;
            let f: u64 = 5u32;
            let g: u64 = small + small;
            let h: u64 = r;
            let mut i: u64 = 1;
            i <<= mid;
            let j: u64 = small as u64;
            let k: u64 = small.into();
            let s = S { v: small };
            s.put(small);
        }};
        let signature = quote!(fn f(big: u64, small: u16, size: usize, mid: u32, r: &u8));
        let declared = quote! { struct S { v: u64 } impl S { fn put(&self, v: u64) {} } };
        assert_eq!(
            expanded(quote!(#declared #[widen] #signature #body)),
            plain(quote!(#declared #signature #body))
        );
    }

    #[test]
    fn a_binding_without_a_written_type_ends_what_is_known() {
        let body = quote! {{
            let n = 1;
            let a: u64 = n;
            let b = |n| { let c: u64 = n; };
            match o { Some(n) => { let d: u64 = n; } None => {} }
            if let Some(n) = o { let e: u64 = n; }
            for n in 0..it.len() { let f: u64 = n; }
        }};
        let input = quote!(#[widen] fn f(n: u32, o: Option<u8>, it: Vec<u8>) #body);
        assert_eq!(
            expanded(input),
            plain(quote!(fn f(n: u32, o: Option<u8>, it: Vec<u8>) #body))
        );
    }

    #[test]
    fn a_name_is_known_again_inside_a_scope_that_types_it_and_not_after() {
        let u64_u8 = from("u64", "u8");
        let u64_u16 = from("u64", "u16");
        assert_eq!(
            expanded(quote! {
                #[widen]
                fn f(n: u8, all: &[u16]) {
                    { let n: u16 = 1; let a: u64 = n; }
                    let b: u64 = n;
                    for v in all { let c: u64 = *v; }
                    let k = |m: u8| { let d: u64 = m; };
                }
            }),
            plain(quote! {
                fn f(n: u8, all: &[u16]) {
                    { let n: u16 = 1; let a: u64 = #u64_u16(n); }
                    let b: u64 = #u64_u8(n);
                    for v in all { let c: u64 = #u64_u16(*v); }
                    let k = |m: u8| { let d: u64 = #u64_u8(m); };
                }
            })
        );
    }

    #[test]
    fn the_attribute_marks_only_what_it_sits_on_and_is_removed() {
        let u64_u8 = from("u64", "u8");
        assert_eq!(
            expanded(quote! {
                #[widen]
                mod m { fn f(n: u8) { let a: u64 = n; fn inner(n: u8) { let b: u64 = n; } } }
                fn g(n: u8) { let c: u64 = n; }
            }),
            plain(quote! {
                mod m {
                    fn f(n: u8) {
                        let a: u64 = #u64_u8(n);
                        fn inner(n: u8) { let b: u64 = #u64_u8(n); }
                    }
                }
                fn g(n: u8) { let c: u64 = n; }
            })
        );
        assert_eq!(
            expanded(quote! { #![widen] fn g(n: u8) { let c: u64 = n; } }),
            plain(quote! { fn g(n: u8) { let c: u64 = #u64_u8(n); } })
        );
    }

    #[test]
    fn a_nested_fn_does_not_see_the_locals_around_it() {
        let input = quote! {
            #[widen]
            fn f(n: u8) { fn inner() { let n = 1; let a: u64 = n; } }
        };
        assert_eq!(
            expanded(input),
            plain(quote!(
                fn f(n: u8) {
                    fn inner() {
                        let n = 1;
                        let a: u64 = n;
                    }
                }
            ))
        );
    }

    #[test]
    fn names_that_mean_different_types_are_left_alone() {
        let input = quote! {
            mod a { pub struct S { pub v: u64 } pub fn put(x: u64) {} }
            mod b { pub struct S { pub v: u32 } pub fn put(x: u32) {} }
            #[widen]
            fn f(n: u8, s: a::S) { put(n); let x: u64 = s.v; s.v = n; }
        };
        assert_eq!(
            expanded(input),
            plain(quote! {
                mod a { pub struct S { pub v: u64 } pub fn put(x: u64) {} }
                mod b { pub struct S { pub v: u32 } pub fn put(x: u32) {} }
                fn f(n: u8, s: a::S) { put(n); let x: u64 = s.v; s.v = n; }
            })
        );
    }

    #[test]
    fn misplaced_repeated_and_argument_attributes_are_reported_together() {
        assert_eq!(
            expanded(quote! {
                #[widen(all)] fn f() {}
                #[widen] #[widen] fn g() {}
                #[widen] struct S;
                impl S { #[widen] const C: u8 = 1; }
            }),
            "`#[widen]` takes no arguments\n\
             `#[widen]` is given twice\n\
             `#[widen]` goes on a fn, an `impl` or a module, or as `#![widen]` at the start of \
             the input\n\
             `#[widen]` goes on a fn, an `impl` or a module, or as `#![widen]` at the start of \
             the input"
        );
    }
}
