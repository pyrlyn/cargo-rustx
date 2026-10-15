//! `#[delegate(Trait)]` fields: a trait impl forwarding every method to the field (problem 07).
//!
//! An AST pass: the struct is valid Rust; method lists come from a built-in table of std traits
//! or from traits declared in the same input.
//!
//! ```text
//! struct Tee<R: Read, W: Write> { #[delegate(Read)] reader: R, log: W }
//! impl<R: Read, W: Write> ::std::io::Read for Tee<R, W> {
//!     #[inline]
//!     fn read(&mut self, __rustx_arg0: &mut [u8]) -> ::std::io::Result<usize> {
//!         ::std::io::Read::read(&mut self.reader, __rustx_arg0)
//!     }
//! }
//! ```
//!
//! The impl copies the struct's generics and `where` clause and goes right after the struct;
//! `inline_bounds` and `#[borrows]` run later and treat it like a hand-written one. Generated
//! tokens take the span of the trait's last segment, so rustc's errors about the impl point at
//! the trait the user named.
//!
//! # Where the items come from
//!
//! A macro sees only its input, so the trait's items are found, first match wins:
//!
//! 1. a list in the attribute, for any trait: `#[delegate(Shape { fn area(&self) -> f64; })]`;
//! 2. a trait declared anywhere in the input and named by a bare identifier: its required items;
//! 3. the std table ([`std_traits`]): `Read`, `Write`, `BufRead`, `Seek`, `Iterator`,
//!    `DoubleEndedIterator`, `Display`, `Debug`, `Hash`, named bare or under `std`, `core`,
//!    `alloc`, `io`, `fmt`, `iter` or `hash`, and implemented by their `::std`/`::core` path.
//!
//! Only required items are forwarded; provided methods are inherited and go through the
//! forwarded ones, so they see hand-written overrides.
//!
//! `except(..)` leaves methods to a hand-written impl; see [`except`].
//!
//! # Limitations
//!
//! - Associated functions without `self`, typed receivers (`self: Box<Self>`) and generic
//!   associated types are reported; a method returning `Self` only compiles if rustc agrees.
//! - A trait declared in the input with generic parameters, or two traits of one name, need the
//!   list form, since the pass neither substitutes arguments nor resolves modules.
//! - Generic methods are called without turbofish: their parameters must be inferable from the
//!   arguments, as they are in `Hash::hash`.
//! - Associated types and consts name the field's type (`<R as Iterator>::Item`), so a
//!   hand-written impl that receives them must spell the struct's parameters as declared. On a
//!   `#[borrows]` struct, elided references in that copy take the lifetime `#[borrows]` adds.

mod except;
mod forward;
mod spec;
mod std_traits;

use std::collections::{HashMap, HashSet};

use proc_macro2::Span;
use quote::{ToTokens, quote_spanned};
use syn::spanned::Spanned;
use syn::visit_mut::{self, VisitMut};
use syn::{Field, Index, Item, ItemImpl, ItemStruct, ItemTrait, Member, Path, TraitItem, Type};

use self::forward::Target;
use self::spec::Delegation;
use super::borrows;
use super::item_lists::{Entry, ExpandList, for_each_list};
use crate::tokens::syntax::{combine, take_attr};

const ATTR: &str = "delegate";

pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    let mut traits = Traits::default();
    traits.visit_file_mut(file);
    let mut walk = Walk {
        traits: traits.0,
        errors: None,
    };
    for_each_list(file, &mut walk);
    walk.errors.map_or(Ok(()), Err)
}

/// Traits declared in the input by name; `None` when two share the name.
#[derive(Default)]
struct Traits(HashMap<String, Option<ItemTrait>>);

impl VisitMut for Traits {
    fn visit_item_trait_mut(&mut self, item: &mut ItemTrait) {
        self.0
            .entry(item.ident.to_string())
            .and_modify(|seen| *seen = None)
            .or_insert_with(|| Some(item.clone()));
        visit_mut::visit_item_trait_mut(self, item);
    }
}

struct Walk {
    traits: HashMap<String, Option<ItemTrait>>,
    errors: Option<syn::Error>,
}

/// One trait of one `#[delegate(..)]`, with the struct and field it belongs to.
struct Request {
    /// Index of the struct in its item list.
    at: usize,
    owner: ItemStruct,
    field: Member,
    ty: Type,
    delegation: Delegation,
}

impl ExpandList for Walk {
    fn expand_list<T: Entry>(&mut self, list: &mut Vec<T>) {
        self.scope(list);
    }
}

impl Walk {
    fn fail(&mut self, err: syn::Error) {
        combine(&mut self.errors, err);
    }

    /// Lowers the structs of one item list. Each impl goes right after its struct, which keeps
    /// it before a block's tail expression.
    fn scope<T: Entry>(&mut self, list: &mut Vec<T>) {
        let mut requests = Vec::new();
        for (at, entry) in list.iter_mut().enumerate() {
            match entry.item_mut() {
                Some(Item::Struct(owner)) => self.collect(at, owner, &mut requests),
                Some(Item::Enum(owner)) => {
                    for variant in &mut owner.variants {
                        self.misplaced(variant.fields.iter_mut(), "an enum variant");
                    }
                }
                Some(Item::Union(owner)) => {
                    self.misplaced(owner.fields.named.iter_mut(), "a union");
                }
                _ => {}
            }
        }
        let mut added = Vec::new();
        for request in requests {
            let existing = list
                .iter_mut()
                .filter_map(Entry::item_mut)
                .find_map(|item| {
                    except::hand_written(item, &request.owner.ident, &request.delegation.path)
                });
            match self.lower(&request, existing) {
                Ok(Some(item)) => added.push((request.at, item)),
                Ok(None) => {}
                Err(err) => self.fail(err),
            }
        }
        // Back to front, so earlier indices stay valid and one struct's impls keep their order.
        for (at, item) in added.into_iter().rev() {
            list.insert(at + 1, T::from_item(item));
        }
    }

    fn collect(&mut self, at: usize, owner: &mut ItemStruct, requests: &mut Vec<Request>) {
        let mut taken = Vec::new();
        for (index, field) in owner.fields.iter_mut().enumerate() {
            let member = field
                .ident
                .clone()
                .map_or_else(|| Member::Unnamed(Index::from(index)), Member::Named);
            while let Some(attr) = take_attr(&mut field.attrs, ATTR) {
                match spec::parse(&attr) {
                    Ok(delegations) => {
                        taken.push((member.clone(), field.ty.clone(), delegations));
                    }
                    Err(err) => self.fail(err),
                }
            }
        }
        // Cloned once the attributes are off, so the impl header sees the struct as rustc will.
        let clean = owner.clone();
        let mut seen: HashMap<String, Member> = HashMap::new();
        for (member, ty, delegations) in taken {
            let ty = named_type(owner, ty);
            for delegation in delegations {
                let key = last_segment(&delegation.path);
                if let Some(first) = seen.get(&key) {
                    let message = format!(
                        "`{key}` is already delegated to field `{}` of `{}`; a trait has one impl",
                        first.to_token_stream(),
                        owner.ident
                    );
                    self.fail(syn::Error::new_spanned(&delegation.path, message));
                    continue;
                }
                seen.insert(key, member.clone());
                requests.push(Request {
                    at,
                    owner: clean.clone(),
                    field: member.clone(),
                    ty: ty.clone(),
                    delegation,
                });
            }
        }
    }

    fn misplaced<'a>(&mut self, fields: impl Iterator<Item = &'a mut Field>, place: &str) {
        for field in fields {
            while let Some(attr) = take_attr(&mut field.attrs, ATTR) {
                let message =
                    format!("`#[delegate(..)]` works on struct fields, not on fields of {place}");
                self.fail(syn::Error::new_spanned(attr, message));
            }
        }
    }

    /// The new impl, or `None` when the items went into a hand-written one.
    fn lower(
        &self,
        request: &Request,
        existing: Option<&mut ItemImpl>,
    ) -> syn::Result<Option<Item>> {
        let delegation = &request.delegation;
        let span = delegation.span();
        let (path, trait_items) = self.trait_items(delegation, span)?;
        except::check(delegation, &trait_items)?;
        let target = Target {
            field: &request.field,
            ty: &request.ty,
            path: &path,
            span,
        };
        let items = forward::items(&target, &trait_items, &delegation.except)?;
        if let Some(existing) = existing {
            except::merge(existing, items)?;
            return Ok(None);
        }
        let owner = &request.owner;
        except::without_impl(delegation, &owner.ident)?;
        let ident = &owner.ident;
        let (impl_generics, type_generics, where_clause) = owner.generics.split_for_impl();
        syn::parse2(quote_spanned!(span=>
            impl #impl_generics #path for #ident #type_generics #where_clause { #(#items)* }
        ))
        .map(Some)
    }

    /// The path the impl names and the items to forward, in the order the module docs list.
    fn trait_items(
        &self,
        delegation: &Delegation,
        span: Span,
    ) -> syn::Result<(Path, Vec<TraitItem>)> {
        let path = &delegation.path;
        let name = last_segment(path);
        if let Some(listed) = &delegation.listed {
            return Ok((path.clone(), listed.clone()));
        }
        let local = path
            .get_ident()
            .and_then(|ident| self.traits.get(&ident.to_string()));
        if let Some(local) = local {
            let Some(local) = local else {
                let message = format!(
                    "several traits named `{name}` are declared in this input; list the methods \
                     to forward: `{name} {{ fn method(&self); }}`"
                );
                return Err(syn::Error::new_spanned(path, message));
            };
            if !local.generics.params.is_empty() {
                let message = format!(
                    "`{name}` has generic parameters; list the methods to forward with the \
                     arguments filled in: `{name}<..> {{ fn method(&self); }}`"
                );
                return Err(syn::Error::new_spanned(path, message));
            }
            let items = local.items.iter().filter(|item| required(item)).cloned();
            return Ok((path.clone(), items.collect()));
        }
        std_traits::lookup(path, span).unwrap_or_else(|| {
            let message = format!(
                "`#[delegate]` does not know the methods of `{name}`: it is neither in the \
                 built-in list of std traits nor declared in this input; list them: \
                 `{name} {{ fn method(&self); }}`"
            );
            Err(syn::Error::new_spanned(path, message))
        })
    }
}

/// The field's type as associated items name it. A `#[borrows]` struct gets its lifetime later,
/// and an elided reference in an associated type is an error, so it takes that lifetime here.
fn named_type(owner: &ItemStruct, mut ty: Type) -> Type {
    if let Some(attr) = owner
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("borrows"))
    {
        let lifetime = borrows::lifetime(attr.path().span());
        borrows::Fill::new(&lifetime, &HashSet::new(), true).visit_type_mut(&mut ty);
    }
    ty
}

/// Whether an impl must provide `item`: a trait item without a default.
fn required(item: &TraitItem) -> bool {
    match item {
        TraitItem::Fn(method) => method.default.is_none(),
        TraitItem::Type(assoc) => assoc.default.is_none(),
        TraitItem::Const(constant) => constant.default.is_none(),
        _ => false,
    }
}

/// The trait's last segment with its arguments: how two mentions of one trait are matched
/// without resolving `use`.
fn last_segment(path: &Path) -> String {
    path.segments
        .last()
        .map(|segment| segment.to_token_stream().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro2::TokenStream;
    use quote::quote;

    fn desugar(sugar: TokenStream) -> syn::Result<String> {
        let mut file: syn::File = syn::parse2(sugar)?;
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
    fn std_trait_is_forwarded_to_a_named_field_right_after_the_struct() {
        assert_eq!(
            desugar(quote! {
                struct Wrapper { #[delegate(Read)] inner: File }
                fn after() {}
            })
            .ok(),
            Some(reprint(quote! {
                struct Wrapper { inner: File }
                impl ::std::io::Read for Wrapper {
                    #[inline]
                    fn read(&mut self, __rustx_arg0: &mut [u8]) -> ::std::io::Result<usize> {
                        ::std::io::Read::read(&mut self.inner, __rustx_arg0)
                    }
                }
                fn after() {}
            }))
        );
    }

    #[test]
    fn generics_and_where_are_copied_and_tuple_fields_are_indexed() {
        assert_eq!(
            desugar(quote! {
                struct Show<'a, T: Clone = u8>(u8, #[delegate(fmt::Display, Debug)] &'a T)
                where T: Copy;
            })
            .ok(),
            Some(reprint(quote! {
                struct Show<'a, T: Clone = u8>(u8, &'a T) where T: Copy;
                impl<'a, T: Clone> ::core::fmt::Display for Show<'a, T> where T: Copy {
                    #[inline]
                    fn fmt(&self, __rustx_arg0: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        ::core::fmt::Display::fmt(&self.1, __rustx_arg0)
                    }
                }
                impl<'a, T: Clone> ::core::fmt::Debug for Show<'a, T> where T: Copy {
                    #[inline]
                    fn fmt(&self, __rustx_arg0: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                        ::core::fmt::Debug::fmt(&self.1, __rustx_arg0)
                    }
                }
            }))
        );
    }

    #[test]
    fn associated_types_and_generic_methods_go_through_the_field_type() {
        assert_eq!(
            desugar(quote! {
                struct Ids { #[delegate(Iterator, std::hash::Hash)] it: Range<u32> }
            })
            .ok(),
            Some(reprint(quote! {
                struct Ids { it: Range<u32> }
                impl ::core::iter::Iterator for Ids {
                    type Item = <Range<u32> as ::core::iter::Iterator>::Item;
                    #[inline]
                    fn next(&mut self) -> ::core::option::Option<Self::Item> {
                        ::core::iter::Iterator::next(&mut self.it )
                    }
                }
                impl ::core::hash::Hash for Ids {
                    #[inline]
                    fn hash<H: ::core::hash::Hasher>(&self, __rustx_arg0: &mut H) {
                        ::core::hash::Hash::hash(&self.it, __rustx_arg0)
                    }
                }
            }))
        );
    }

    #[test]
    fn except_adds_the_other_methods_to_the_hand_written_impl() {
        assert_eq!(
            desugar(quote! {
                struct Counting<W: Write> { #[delegate(Write, except(write))] inner: W, n: usize }
                impl Write for Counting<W> {
                    fn write(&mut self, buf: &[u8]) -> io::Result<usize> { self.inner.write(buf) }
                }
            })
            .ok(),
            Some(reprint(quote! {
                struct Counting<W: Write> { inner: W, n: usize }
                impl Write for Counting<W> {
                    fn write(&mut self, buf: &[u8]) -> io::Result<usize> { self.inner.write(buf) }
                    #[inline]
                    fn flush(&mut self) -> ::std::io::Result<()> {
                        ::std::io::Write::flush(&mut self.inner )
                    }
                }
            }))
        );
    }

    #[test]
    fn a_trait_of_the_input_forwards_its_required_items_with_every_receiver() {
        assert_eq!(
            desugar(quote! {
                trait Shape {
                    const SIDES: u8;
                    fn area(&self) -> f64;
                    fn grow(&mut self, (by, _): (f64, u8));
                    fn into_name(mut self) -> String;
                    fn sides(&self) -> u8 { Self::SIDES }
                }
                mod inner { struct Tile { #[delegate(Shape)] shape: Square } }
            })
            .ok(),
            Some(reprint(quote! {
                trait Shape {
                    const SIDES: u8;
                    fn area(&self) -> f64;
                    fn grow(&mut self, (by, _): (f64, u8));
                    fn into_name(mut self) -> String;
                    fn sides(&self) -> u8 { Self::SIDES }
                }
                mod inner {
                    struct Tile { shape: Square }
                    impl Shape for Tile {
                        const SIDES: u8 = <Square as Shape>::SIDES;
                        #[inline]
                        fn area(&self) -> f64 { Shape::area(&self.shape ) }
                        #[inline]
                        fn grow(&mut self, __rustx_arg0: (f64, u8)) {
                            Shape::grow(&mut self.shape, __rustx_arg0)
                        }
                        #[inline]
                        fn into_name(self) -> String { Shape::into_name(self.shape ) }
                    }
                }
            }))
        );
    }

    #[test]
    fn a_listed_trait_needs_no_declaration_and_wins_over_the_table() {
        assert_eq!(
            desugar(quote! {
                struct Name { #[delegate(AsRef<str> { fn as_ref(&self) -> &str; })] s: String }
                struct Raw { #[delegate(Read { fn read(&mut self, b: &mut [u8]) -> R; })] f: F }
            })
            .ok(),
            Some(reprint(quote! {
                struct Name { s: String }
                impl AsRef<str> for Name {
                    #[inline]
                    fn as_ref(&self) -> &str { AsRef::<str>::as_ref(&self.s ) }
                }
                struct Raw { f: F }
                impl Read for Raw {
                    #[inline]
                    fn read(&mut self, __rustx_arg0: &mut [u8]) -> R {
                        Read::read(&mut self.f, __rustx_arg0)
                    }
                }
            }))
        );
    }

    #[test]
    fn a_borrows_struct_gets_its_lifetime_in_the_impl_and_the_associated_type() {
        let expanded = crate::try_expand(quote! {
            #[borrows]
            struct Words { #[delegate(Iterator)] it: SplitWhitespace<'_>, src: &str }
        })
        .map(|file| file.into_token_stream().to_string());
        assert_eq!(
            expanded.ok(),
            Some(reprint(quote! {
                struct Words<'__rustx_borrows> {
                    it: SplitWhitespace<'__rustx_borrows>,
                    src: &'__rustx_borrows str
                }
                impl<'__rustx_borrows> ::core::iter::Iterator for Words<'__rustx_borrows> {
                    type Item = <SplitWhitespace<'__rustx_borrows> as ::core::iter::Iterator>::Item;
                    #[inline]
                    fn next(&mut self) -> ::core::option::Option<Self::Item> {
                        ::core::iter::Iterator::next(&mut self.it)
                    }
                }
            }))
        );
    }

    #[test]
    fn impls_of_a_struct_in_a_block_come_before_the_tail_expression() {
        let expanded = desugar(quote! {
            fn f() -> u8 { struct W { #[delegate(Debug)] x: u8 } 1 }
        })
        .unwrap_or_default();
        let tail = expanded.rfind(" 1 }").unwrap_or_default();
        assert!(
            expanded.find("impl").is_some_and(|at| at < tail),
            "{expanded}"
        );
    }

    #[test]
    fn valid_rust_passes_through_unchanged() {
        let valid = quote! {
            #[derive(Debug)] struct S { #[serde(skip)] a: u8 }
            impl ::std::io::Write for S { fn write(&mut self, b: &[u8]) -> R { f() } }
            trait Read { fn read(&self); }
        };
        assert_eq!(desugar(valid.clone()).ok(), Some(reprint(valid)));
    }

    #[test]
    fn unknown_trait_without_a_list_is_an_error() {
        let errors = errors(quote! {
            struct S { #[delegate(Shape)] a: u8, #[delegate(my::Read)] b: u8 }
        });
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors[0].starts_with("`#[delegate]` does not know the methods of `Shape`"));
        assert!(errors[1].contains("methods of `Read`"), "{errors:?}");
    }

    #[test]
    fn delegate_outside_struct_fields_is_an_error() {
        let errors = errors(quote! {
            enum E { A(#[delegate(Read)] File), B { #[delegate(Write)] w: File } }
            union U { #[delegate(Debug)] a: u8 }
        });
        assert_eq!(
            errors,
            [
                "`#[delegate(..)]` works on struct fields, not on fields of an enum variant",
                "`#[delegate(..)]` works on struct fields, not on fields of an enum variant",
                "`#[delegate(..)]` works on struct fields, not on fields of a union",
            ]
        );
    }

    #[test]
    fn two_fields_delegating_one_trait_is_an_error() {
        let errors = errors(quote! {
            struct Tee { #[delegate(Write)] a: File, #[delegate(io::Write, Read)] b: File }
        });
        assert_eq!(
            errors,
            ["`Write` is already delegated to field `a` of `Tee`; a trait has one impl"]
        );
    }

    #[test]
    fn except_misuse_is_an_error() {
        let errors = errors(quote! {
            struct A { #[delegate(Write, except(write))] w: File }
            struct B { #[delegate(Read, except(read_exact))] r: File }
            struct C { #[delegate(except(read), Read)] r: File }
            struct D { #[delegate(Write)] w: File }
            impl Write for D { fn flush(&mut self) -> R { g() } }
        });
        assert_eq!(
            errors,
            [
                "`except(..)` must follow the trait it applies to, as in `Write, except(write)`",
                "`except(..)` needs a hand-written `impl Write for A` next to the struct",
                "`read_exact` is not a forwarded method of `Read`",
                "`flush` is written by hand and also delegated; add it to `except(..)`",
            ]
        );
    }

    #[test]
    fn methods_that_cannot_be_forwarded_are_errors() {
        let errors = errors(quote! {
            trait Make { fn new() -> Self; }
            trait Boxed { fn take(self: Box<Self>); }
            trait Gat { type It<'a>; }
            struct S { #[delegate(Make)] a: u8, #[delegate(Boxed)] b: u8, #[delegate(Gat)] c: u8 }
        });
        assert_eq!(
            errors,
            [
                "`new` takes no `self`, so it cannot be forwarded to a field",
                "only `self`, `&self` and `&mut self` methods are delegated",
                "generic associated types are not delegated; implement the trait by hand",
            ]
        );
    }

    #[test]
    fn traits_the_pass_cannot_read_need_a_list() {
        let errors = errors(quote! {
            mod a { trait Twice { fn f(&self); } }
            mod b { trait Twice { fn g(&self); } }
            trait Conv<T> { fn conv(&self) -> T; }
            struct S { #[delegate(Twice)] a: u8, #[delegate(Conv)] b: u8 }
        });
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(
            errors[0].starts_with("several traits named `Twice`"),
            "{errors:?}"
        );
        assert!(
            errors[1].starts_with("`Conv` has generic parameters"),
            "{errors:?}"
        );
    }

    #[test]
    fn bad_attribute_syntax_is_an_error() {
        let errors = errors(quote! {
            struct S { #[delegate()] a: u8, #[delegate] b: u8, #[delegate(Read Write)] c: u8 }
        });
        assert_eq!(errors.len(), 3, "{errors:?}");
        assert!(errors[0].starts_with("`#[delegate(..)]` needs at least one trait"));
    }
}
