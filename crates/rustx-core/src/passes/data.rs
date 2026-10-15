//! `#[data]` structs: constructor, getters and an optional builder (problem 08).
//!
//! An AST pass: the struct is valid Rust, so it only needs the parsed fields. It runs before
//! `inline_bounds` and `borrows`, so the impls it generates are extended like hand-written ones.
//!
//! `#[data]` on a struct with named fields, at any depth (modules, fn bodies), adds one `impl`
//! right after the struct, with the struct's generics and `where` clause:
//!
//! - `pub fn new(..) -> Self` taking every field in order, except fields marked
//!   `#[default(expr)]`, which are initialised with `expr`;
//! - a getter per field, named after it: by value for `Copy` primitives (`bool`, `char`, the
//!   integer and float types) and shared references, `&str` for `String`, `&T` otherwise.
//!
//! Flags in `#[data(..)]` add more:
//!
//! - `setters`: `pub fn set_<field>(&mut self, <field>: T)` per field;
//! - `with`: `pub fn with_<field>(mut self, <field>: T) -> Self` per field;
//! - `builder`: `pub fn builder() -> <Name>Builder`, a builder struct with the struct's
//!   visibility and generics, one `fn <field>(mut self, v: T) -> Self` per field, and
//!   `fn build(self) -> Result<Name, &'static str>`, which fails on a missing field that has no
//!   `#[default(..)]`.
//!
//! Generated methods are `pub`, as in the problem file, and carry `#[allow(dead_code)]` on their
//! `impl`: the user asked for the set, not for each method, so unused ones are not warnings.
//!
//! On a `#[borrows]` struct the parameters and by-value getters name the struct's lifetime
//! instead of eliding it, because an elided parameter lifetime could not be stored in the struct.
//! The builder struct takes `#[borrows]` too.
//!
//! # Limitations
//!
//! - `Copy` is judged by the written type: a user `Copy` type gets a `&T` getter.
//! - `#[cfg]` on a field is not carried to the generated parameters and methods.
//! - Field defaults written as `field: T = e` (problem 17) are not read: they are lowered
//!   before this pass, so `new` still takes such a field.

mod builder;
mod methods;

use std::collections::HashSet;
use std::mem;

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::visit_mut::{self, VisitMut};
use syn::{Attribute, Block, Expr, Fields, Ident, Item, ItemMod, ItemStruct, Lifetime, Meta, Stmt};

use super::borrows;
use crate::tokens::syntax::{combine, take_attr};

const ATTR: &str = "data";
const DEFAULT: &str = "default";

pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    let mut pass = Lower { errors: None };
    pass.visit_file_mut(file);
    pass.errors.map_or(Ok(()), Err)
}

/// What `#[data(..)]` asks for beyond `new` and getters.
#[derive(Default)]
struct Flags {
    setters: bool,
    with: bool,
    /// The `builder` token, which names the builder.
    builder: Option<Span>,
}

/// One field of a `#[data]` struct.
struct DataField {
    ident: Ident,
    ty: syn::Type,
    default: Option<Expr>,
}

/// A `#[data]` struct with its markers taken off, ready to generate from.
struct Data<'a> {
    item: &'a ItemStruct,
    /// The `data` token, where the generated `impl` points.
    span: Span,
    flags: Flags,
    fields: Vec<DataField>,
    /// The lifetime `#[borrows]` will add, for types copied into signatures.
    lifetime: Option<Lifetime>,
}

impl Data<'_> {
    /// `ty` as a parameter or by-value result: elided references name the struct's lifetime.
    fn signature_type(&self, ty: &syn::Type) -> syn::Type {
        let mut ty = ty.clone();
        if let Some(lifetime) = &self.lifetime {
            let types = HashSet::new();
            borrows::Fill::new(lifetime, &types, true).visit_type_mut(&mut ty);
        }
        ty
    }
}

struct Lower {
    errors: Option<syn::Error>,
}

impl Lower {
    /// Expands every `#[data]` item of one item list, putting the generated items after it.
    fn expand_list<T>(
        &mut self,
        list: &mut Vec<T>,
        as_item: fn(&mut T) -> Option<&mut Item>,
        wrap: fn(Item) -> T,
    ) {
        // A builder must not collide with a type declared next to the struct.
        let declared: HashSet<String> = list
            .iter_mut()
            .filter_map(as_item)
            .filter_map(|item| type_name(item))
            .collect();
        let mut out = Vec::with_capacity(list.len());
        for mut entry in mem::take(list) {
            let generated = match as_item(&mut entry).map(|item| expand(item, &declared)) {
                Some(Ok(generated)) => generated,
                Some(Err(err)) => {
                    combine(&mut self.errors, err);
                    Vec::new()
                }
                None => Vec::new(),
            };
            out.push(entry);
            out.extend(generated.into_iter().map(wrap));
        }
        *list = out;
    }
}

impl VisitMut for Lower {
    fn visit_file_mut(&mut self, file: &mut syn::File) {
        self.expand_list(&mut file.items, item_of_item, |item| item);
        visit_mut::visit_file_mut(self, file);
    }

    fn visit_item_mod_mut(&mut self, module: &mut ItemMod) {
        if let Some((_, items)) = &mut module.content {
            self.expand_list(items, item_of_item, |item| item);
        }
        visit_mut::visit_item_mod_mut(self, module);
    }

    fn visit_block_mut(&mut self, block: &mut Block) {
        self.expand_list(&mut block.stmts, item_of_stmt, Stmt::Item);
        visit_mut::visit_block_mut(self, block);
    }
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "shaped like `item_of_stmt`, so both fit `expand_list`"
)]
fn item_of_item(item: &mut Item) -> Option<&mut Item> {
    Some(item)
}

fn item_of_stmt(stmt: &mut Stmt) -> Option<&mut Item> {
    match stmt {
        Stmt::Item(item) => Some(item),
        _ => None,
    }
}

fn type_name(item: &Item) -> Option<String> {
    let ident = match item {
        Item::Struct(item) => &item.ident,
        Item::Enum(item) => &item.ident,
        Item::Union(item) => &item.ident,
        Item::Type(item) => &item.ident,
        Item::Trait(item) => &item.ident,
        Item::TraitAlias(item) => &item.ident,
        _ => return None,
    };
    Some(ident.to_string())
}

/// The items `#[data]` on `item` generates, after taking the markers off.
fn expand(item: &mut Item, declared: &HashSet<String>) -> syn::Result<Vec<Item>> {
    let (attrs, kind) = match item {
        Item::Struct(item) => return expand_struct(item, declared),
        Item::Enum(item) => (&mut item.attrs, "an enum"),
        Item::Union(item) => (&mut item.attrs, "a union"),
        _ => return Ok(Vec::new()),
    };
    match take_attr(attrs, ATTR) {
        Some(attr) => Err(syn::Error::new(
            attr.path().span(),
            format!("`#[data]` needs a struct with named fields, not {kind}"),
        )),
        None => Ok(Vec::new()),
    }
}

fn expand_struct(item: &mut ItemStruct, declared: &HashSet<String>) -> syn::Result<Vec<Item>> {
    let Some(attr) = take_attr(&mut item.attrs, ATTR) else {
        return Ok(Vec::new());
    };
    let span = attr.path().span();
    let mut errors = None;
    while let Some(again) = take_attr(&mut item.attrs, ATTR) {
        let err = syn::Error::new(again.path().span(), "`#[data]` is given twice");
        combine(&mut errors, err);
    }
    let flags = flags(&attr).unwrap_or_else(|err| {
        combine(&mut errors, err);
        Flags::default()
    });
    let Fields::Named(named) = &mut item.fields else {
        let kind = if matches!(item.fields, Fields::Unit) {
            "a unit struct"
        } else {
            "a tuple struct"
        };
        let msg = format!("`#[data]` needs a struct with named fields, not {kind}");
        return Err(syn::Error::new(span, msg));
    };
    let fields: Vec<DataField> = named
        .named
        .iter_mut()
        .filter_map(|field| {
            let default = take_default(&mut field.attrs)
                .map_err(|err| combine(&mut errors, err))
                .ok()?;
            Some(DataField {
                ident: field.ident.clone()?,
                ty: field.ty.clone(),
                default,
            })
        })
        .collect();
    if let Some(at) = flags.builder
        && let Err(err) = builder::check(&item.ident, &fields, at, declared)
    {
        combine(&mut errors, err);
    }
    if let Some(errors) = errors {
        return Err(errors);
    }
    let lifetime = item
        .attrs
        .iter()
        .find(|attr| attr.path().is_ident("borrows"))
        .map(|attr| borrows::lifetime(attr.path().span()));
    let data = Data {
        item,
        span,
        flags,
        fields,
        lifetime,
    };
    let mut items = vec![methods::generate(&data)?];
    items.extend(builder::generate(&data)?);
    Ok(items)
}

fn flags(attr: &Attribute) -> syn::Result<Flags> {
    let mut flags = Flags::default();
    match &attr.meta {
        Meta::Path(_) => return Ok(flags),
        Meta::NameValue(meta) => {
            let msg = "`#[data]` takes flags in parentheses: `#[data(setters, with, builder)]`";
            return Err(syn::Error::new_spanned(&meta.value, msg));
        }
        Meta::List(_) => {}
    }
    attr.parse_nested_meta(|meta| {
        let name = meta.path.get_ident().map(ToString::to_string);
        let given = match name.as_deref() {
            Some("setters") => mem::replace(&mut flags.setters, true),
            Some("with") => mem::replace(&mut flags.with, true),
            Some("builder") => flags.builder.replace(meta.path.span()).is_some(),
            _ => {
                let msg = "unknown `#[data]` flag; expected `setters`, `with` or `builder`";
                return Err(meta.error(msg));
            }
        };
        if given {
            return Err(meta.error("this `#[data]` flag is given twice"));
        }
        if !meta.input.is_empty() && !meta.input.peek(syn::Token![,]) {
            return Err(meta.error("a `#[data]` flag takes no value"));
        }
        Ok(())
    })?;
    Ok(flags)
}

/// Takes `#[default(expr)]` off a field.
fn take_default(attrs: &mut Vec<Attribute>) -> syn::Result<Option<Expr>> {
    let Some(attr) = take_attr(attrs, DEFAULT) else {
        return Ok(None);
    };
    if let Some(again) = take_attr(attrs, DEFAULT) {
        let msg = "a field takes one `#[default(..)]`";
        return Err(syn::Error::new(again.path().span(), msg));
    }
    if let Meta::List(_) = attr.meta {
        attr.parse_args().map(Some)
    } else {
        let msg = "`#[default]` on a `#[data]` field needs a value: `#[default(expr)]`";
        Err(syn::Error::new(attr.path().span(), msg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::{ToTokens, quote};

    pub(super) fn lowered(source: proc_macro2::TokenStream) -> syn::Result<String> {
        let mut file: syn::File = syn::parse2(source)?;
        lower(&mut file)?;
        Ok(file.into_token_stream().to_string())
    }

    pub(super) fn printed(plain: proc_macro2::TokenStream) -> String {
        syn::parse2::<syn::File>(plain)
            .unwrap()
            .into_token_stream()
            .to_string()
    }

    pub(super) fn errors(source: proc_macro2::TokenStream) -> Vec<String> {
        lowered(source)
            .unwrap_err()
            .into_iter()
            .map(|err| err.to_string())
            .collect()
    }

    #[test]
    fn data_generates_new_and_getters_after_the_struct() {
        assert_eq!(
            lowered(quote! {
                #[data]
                #[derive(Debug)]
                struct User { name: String, age: u32, tags: Vec<String> }
                fn after() {}
            })
            .unwrap(),
            printed(quote! {
                #[derive(Debug)]
                struct User { name: String, age: u32, tags: Vec<String> }
                #[allow(dead_code)]
                impl User {
                    pub fn new(name: String, age: u32, tags: Vec<String>) -> Self {
                        Self { name, age, tags }
                    }
                    pub fn name(&self) -> &str { &self.name }
                    pub fn age(&self) -> u32 { self.age }
                    pub fn tags(&self) -> &Vec<String> { &self.tags }
                }
                fn after() {}
            })
        );
    }

    #[test]
    fn structs_in_modules_and_fn_bodies_are_expanded() {
        let expanded = lowered(quote! {
            mod m { #[data] pub struct A { a: u8 } }
            fn f() { #[data] struct B { b: u8 } let x = 1; }
        })
        .unwrap();
        assert!(expanded.contains("impl A"), "{expanded}");
        assert!(expanded.contains("impl B"), "{expanded}");
        assert!(!expanded.contains("data"), "{expanded}");
    }

    #[test]
    fn valid_rust_is_unchanged() {
        let plain = quote! {
            #[derive(Debug, Default)]
            struct S { #[serde(default)] a: u8 }
            enum E { #[default] A }
            mod m { fn g() { struct T(u8); } }
        };
        assert_eq!(lowered(plain.clone()).unwrap(), printed(plain));
    }

    #[test]
    fn data_on_other_items_is_an_error_and_errors_are_combined() {
        assert_eq!(
            errors(quote! {
                #[data] struct T(u8);
                #[data] struct U;
                #[data] enum E { A }
                #[data] union N { a: u8 }
                mod m { #[data(getters)] struct S { a: u8 } }
            }),
            [
                "`#[data]` needs a struct with named fields, not a tuple struct",
                "`#[data]` needs a struct with named fields, not a unit struct",
                "`#[data]` needs a struct with named fields, not an enum",
                "`#[data]` needs a struct with named fields, not a union",
                "unknown `#[data]` flag; expected `setters`, `with` or `builder`",
            ]
        );
    }

    #[test]
    fn flags_must_be_known_given_once_and_without_values() {
        for (flags, msg) in [
            (
                quote!((setters, setters)),
                "this `#[data]` flag is given twice",
            ),
            (quote!((with = true)), "a `#[data]` flag takes no value"),
            (
                quote!((a::b)),
                "unknown `#[data]` flag; expected `setters`, `with` or `builder`",
            ),
            (
                quote!(= "x"),
                "`#[data]` takes flags in parentheses: `#[data(setters, with, builder)]`",
            ),
        ] {
            assert_eq!(errors(quote! { #[data #flags] struct S { a: u8 } }), [msg]);
        }
    }

    #[test]
    fn field_and_attribute_errors_of_one_struct_are_combined() {
        assert_eq!(
            errors(quote! {
                #[data(nope)]
                #[data]
                struct S { #[default] a: u8, #[default(1)] #[default(2)] b: u8 }
            }),
            [
                "`#[data]` is given twice",
                "unknown `#[data]` flag; expected `setters`, `with` or `builder`",
                "`#[default]` on a `#[data]` field needs a value: `#[default(expr)]`",
                "a field takes one `#[default(..)]`",
            ]
        );
    }
}
