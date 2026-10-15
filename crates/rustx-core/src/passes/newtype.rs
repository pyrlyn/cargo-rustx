//! `#[newtype(deref, display, from_str, validate = check)]` on a one-field tuple struct: `new`
//! plus one impl per listed flag (problem 20).
//!
//! An AST pass: the struct is valid Rust. It runs before `inline_bounds` and `#[borrows]`, so the
//! impls it generates are extended like hand-written ones.
//!
//! A `#[newtype]` struct, at any depth (modules, fn bodies), gets right after it an inherent
//! `impl` with `pub fn new(inner) -> Self`, and one impl per flag. Every impl copies the struct's
//! generics and `where` clause and adds no bounds of its own, so a generic newtype states what
//! its flags need (`T: Display` for `display`):
//!
//! - `deref`: `Deref` with `Target = str` for a `String`, `[T]` for a `Vec<T>`, the inner type
//!   otherwise. Never `DerefMut`: nothing is generated that the attribute does not name;
//! - `as_ref`: `AsRef` of the same target;
//! - `display`: `Display` forwarding to the inner value, flags included; `display = "{} m"`
//!   writes that string instead, with `{}` or `{0}` for the inner value;
//! - `from_str`: `FromStr` through the inner type's `FromStr`, with its `Err`;
//! - `into`: `From<Newtype> for Inner`;
//! - `add`, `sub`, `mul`, `div`, `rem`: the operator on two newtypes, applied to the inner values.
//!
//! # `validate = f`
//!
//! With `f: fn(&Inner) -> Result<(), E>`, `new` returns `Result<Self, E>` after calling `f`, and
//! `from_str` builds through `new` with `type Err = E`, so no path skips the check. A macro sees
//! no types, so `E` is read from `f`'s signature: `f` must be a function declared in the same
//! item list as the struct. A `String` is built from the `&str` directly, as in problem 20; any
//! other inner type is parsed first, and its parse error must convert into `E` with `From`. The
//! operator flags would build unchecked values, so they are an error together with `validate`.

mod impls;

use std::collections::HashMap;

use proc_macro2::Span;
use syn::spanned::Spanned;
use syn::{
    Attribute, Fields, GenericArgument, Ident, Item, ItemStruct, LitStr, Path, PathArguments,
    ReturnType, Token, Type,
};

use super::attr_flags::parse_flags;
use super::from_display::format;
use super::item_lists::{Entry, ExpandList, for_each_list, insert_after};
use crate::tokens::syntax::{combine, take_attr};

const ATTR: &str = "newtype";

/// What a flag generates; an operator carries its `core::ops` trait.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Deref,
    AsRef,
    Display,
    FromStr,
    Into,
    Operator(&'static str),
}

const FLAGS: &[(&str, Kind)] = &[
    ("deref", Kind::Deref),
    ("as_ref", Kind::AsRef),
    ("display", Kind::Display),
    ("from_str", Kind::FromStr),
    ("into", Kind::Into),
    ("add", Kind::Operator("Add")),
    ("sub", Kind::Operator("Sub")),
    ("mul", Kind::Operator("Mul")),
    ("div", Kind::Operator("Div")),
    ("rem", Kind::Operator("Rem")),
];

const UNKNOWN: &str = "unknown `#[newtype]` flag; expected `deref`, `as_ref`, `display`, \
                       `from_str`, `into`, `add`, `sub`, `mul`, `div`, `rem` or `validate`";
const VALIDATE_VALUE: &str = "`validate` needs a function: `validate = check`";
const DISPLAY_VALUE: &str = "`display` takes a format string: `display = \"{} m\"`";

pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    let mut pass = Lower { errors: None };
    for_each_list(file, &mut pass);
    pass.errors.map_or(Ok(()), Err)
}

struct Lower {
    errors: Option<syn::Error>,
}

impl ExpandList for Lower {
    fn expand_list<T: Entry>(&mut self, list: &mut Vec<T>) {
        let functions = functions(list);
        insert_after(list, |item| {
            expand(item, &functions).unwrap_or_else(|err| {
                combine(&mut self.errors, err);
                Vec::new()
            })
        });
    }
}

/// One flag as written.
struct Flag {
    kind: Kind,
    name: Ident,
    /// `display = ".."`, rewritten to name the inner value as argument `0`, and whether it does.
    format: Option<(LitStr, bool)>,
}

#[derive(Default)]
struct Flags {
    list: Vec<Flag>,
    validate: Option<Path>,
}

/// A `#[newtype]` struct with its attribute taken off, ready to generate from.
struct Newtype<'a> {
    item: &'a ItemStruct,
    /// The `newtype` token, where `new` points.
    span: Span,
    inner: &'a Type,
    /// The validator and the error type `E` of its `Result<(), E>`.
    validate: Option<(&'a Path, &'a Type)>,
}

/// The return types of the functions of one item list, by name: where `validate = f` finds `E`.
fn functions<T: Entry>(list: &mut [T]) -> HashMap<String, ReturnType> {
    list.iter_mut()
        .filter_map(|entry| match entry.item_mut()? {
            Item::Fn(f) => Some((f.sig.ident.to_string(), f.sig.output.clone())),
            _ => None,
        })
        .collect()
}

/// The items `#[newtype]` on `item` generates, after taking the attribute off.
fn expand(item: &mut Item, functions: &HashMap<String, ReturnType>) -> syn::Result<Vec<Item>> {
    let (attrs, kind) = match item {
        Item::Struct(item) => return expand_struct(item, functions),
        Item::Enum(item) => (&mut item.attrs, "an enum"),
        Item::Union(item) => (&mut item.attrs, "a union"),
        _ => return Ok(Vec::new()),
    };
    match take_attr(attrs, ATTR) {
        Some(attr) => Err(shape_error(attr.path().span(), kind)),
        None => Ok(Vec::new()),
    }
}

fn shape_error(span: Span, kind: &str) -> syn::Error {
    let msg = format!("`#[newtype]` needs a tuple struct with one field, not {kind}");
    syn::Error::new(span, msg)
}

fn expand_struct(
    item: &mut ItemStruct,
    functions: &HashMap<String, ReturnType>,
) -> syn::Result<Vec<Item>> {
    let Some(attr) = take_attr(&mut item.attrs, ATTR) else {
        return Ok(Vec::new());
    };
    let span = attr.path().span();
    let mut errors = None;
    while let Some(again) = take_attr(&mut item.attrs, ATTR) {
        let err = syn::Error::new(again.path().span(), "`#[newtype]` is given twice");
        combine(&mut errors, err);
    }
    let item: &ItemStruct = item;
    let inner = match &item.fields {
        Fields::Unnamed(fields) if fields.unnamed.len() == 1 => &fields.unnamed[0].ty,
        Fields::Unnamed(fields) => {
            let kind = format!("a tuple struct with {} fields", fields.unnamed.len());
            return Err(shape_error(span, &kind));
        }
        Fields::Named(_) => return Err(shape_error(span, "a struct with named fields")),
        Fields::Unit => return Err(shape_error(span, "a unit struct")),
    };
    let flags = flags(&attr, &item.ident).unwrap_or_else(|err| {
        combine(&mut errors, err);
        Flags::default()
    });
    let validate = flags.validate.as_ref().and_then(|path| {
        let error = validator_error(path, functions);
        error
            .map_err(|err| combine(&mut errors, err))
            .ok()
            .map(|error| (path, error))
    });
    if flags.validate.is_some() {
        for flag in &flags.list {
            if let Kind::Operator(_) = flag.kind {
                let msg = format!(
                    "`{}` would build a `{}` that `validate` never checked; operators cannot be \
                     combined with `validate`",
                    flag.name, item.ident
                );
                combine(&mut errors, syn::Error::new(flag.name.span(), msg));
            }
        }
    }
    if let Some(errors) = errors {
        return Err(errors);
    }
    let newtype = Newtype {
        item,
        span,
        inner,
        validate,
    };
    impls::generate(&newtype, &flags.list)
}

fn flags(attr: &Attribute, name: &Ident) -> syn::Result<Flags> {
    let mut flags = Flags::default();
    parse_flags(attr, "#[newtype(deref, display)]", |meta| {
        let flag = meta.path.get_ident().ok_or_else(|| meta.error(UNKNOWN))?;
        let has_value = meta.input.peek(Token![=]);
        if flag == "validate" {
            if !has_value {
                return Err(meta.error(VALIDATE_VALUE));
            }
            flags.validate = Some(meta.value()?.parse()?);
            return Ok(true);
        }
        let Some(&(_, kind)) = FLAGS.iter().find(|(known, _)| flag == known) else {
            return Err(meta.error(UNKNOWN));
        };
        let format = if kind == Kind::Display && has_value {
            let lit: LitStr = meta
                .value()?
                .parse()
                .map_err(|err| syn::Error::new(err.span(), DISPLAY_VALUE))?;
            Some(display_format(&lit, name)?)
        } else {
            None
        };
        let read_value = format.is_some();
        flags.list.push(Flag {
            kind,
            name: flag.clone(),
            format,
        });
        Ok(read_value)
    })?;
    Ok(flags)
}

/// `display = ".."` with its placeholders renumbered by the `#[display]` rules of problem 05: the
/// inner value is field `0`.
fn display_format(lit: &LitStr, name: &Ident) -> syn::Result<(LitStr, bool)> {
    let (text, used) = format::rewrite(&lit.value(), &name.to_string(), |arg| {
        (arg == "0").then_some(0)
    })
    .map_err(|msg| syn::Error::new(lit.span(), msg))?;
    Ok((LitStr::new(&text, lit.span()), !used.is_empty()))
}

/// `E` of the `Result<(), E>` that the function `validate = path` names returns.
fn validator_error<'a>(
    path: &Path,
    functions: &'a HashMap<String, ReturnType>,
) -> syn::Result<&'a Type> {
    let Some(output) = path
        .get_ident()
        .and_then(|name| functions.get(&name.to_string()))
    else {
        let msg = "`validate` needs a function declared next to the struct, as in \
                   `validate = check`, so `new` can name its error type";
        return Err(syn::Error::new_spanned(path, msg));
    };
    let msg = "a `validate` function returns `Result<(), E>`";
    let ReturnType::Type(_, ty) = output else {
        return Err(syn::Error::new_spanned(path, msg));
    };
    result_error(ty).ok_or_else(|| syn::Error::new_spanned(ty, msg))
}

fn result_error(ty: &Type) -> Option<&Type> {
    let Type::Path(path) = ty else {
        return None;
    };
    let last = path.path.segments.last()?;
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };
    let args: Vec<&GenericArgument> = args.args.iter().collect();
    match args[..] {
        [
            GenericArgument::Type(Type::Tuple(unit)),
            GenericArgument::Type(error),
        ] if last.ident == "Result" && unit.elems.is_empty() => Some(error),
        _ => None,
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

    fn errors(source: proc_macro2::TokenStream) -> Vec<String> {
        lowered(source)
            .unwrap_err()
            .into_iter()
            .map(|err| err.to_string())
            .collect()
    }

    #[test]
    fn a_bare_newtype_gets_only_an_infallible_new() {
        assert_eq!(
            lowered(quote! {
                #[newtype]
                #[derive(Debug)]
                struct Id(u64);
                fn after() {}
            })
            .unwrap(),
            printed(quote! {
                #[derive(Debug)]
                struct Id(u64);
                #[allow(dead_code)]
                impl Id {
                    pub fn new(__rustx_inner: u64) -> Self { Self(__rustx_inner) }
                }
                fn after() {}
            })
        );
    }

    #[test]
    fn structs_in_modules_and_fn_bodies_are_expanded() {
        let expanded = lowered(quote! {
            mod m { #[newtype(deref)] pub struct A(u8); }
            fn f() { #[newtype(into)] struct B(u8); let x = 1; }
        })
        .unwrap();
        assert!(expanded.contains("Deref for A"), "{expanded}");
        assert!(expanded.contains("From < B > for u8"), "{expanded}");
        assert!(
            expanded.find("for u8").unwrap() < expanded.find("let x").unwrap(),
            "{expanded}"
        );
        assert!(!expanded.contains("newtype"), "{expanded}");
    }

    #[test]
    fn valid_rust_is_unchanged() {
        let plain = quote! {
            #[derive(Debug, Default)]
            struct S(#[serde(default)] u8);
            fn check(s: &String) -> Result<(), String> { Ok(()) }
            enum E { A(u8) }
            mod m { fn g() { struct T(u8); } }
        };
        assert_eq!(lowered(plain.clone()).unwrap(), printed(plain));
    }

    #[test]
    fn newtype_on_other_items_is_an_error_and_errors_are_combined() {
        assert_eq!(
            errors(quote! {
                #[newtype] struct A { a: u8 }
                #[newtype] struct B;
                #[newtype] struct C(u8, u8);
                #[newtype] enum E { A }
                #[newtype] union N { a: u8 }
                mod m { #[newtype(deref_mut)] struct S(u8); }
            }),
            [
                "`#[newtype]` needs a tuple struct with one field, not a struct with named fields",
                "`#[newtype]` needs a tuple struct with one field, not a unit struct",
                "`#[newtype]` needs a tuple struct with one field, not a tuple struct with 2 fields",
                "`#[newtype]` needs a tuple struct with one field, not an enum",
                "`#[newtype]` needs a tuple struct with one field, not a union",
                UNKNOWN,
            ]
        );
    }

    #[test]
    fn flags_must_be_known_given_once_and_valued_only_where_allowed() {
        for (flags, msg) in [
            (
                quote!((deref, deref)),
                "this `#[newtype]` flag is given twice",
            ),
            (
                quote!((deref = true)),
                "the `#[newtype]` flag `deref` takes no value",
            ),
            (quote!((validate)), VALIDATE_VALUE),
            (quote!((validate(check))), VALIDATE_VALUE),
            (quote!((display = 1)), DISPLAY_VALUE),
            (quote!((a::b)), UNKNOWN),
            (quote!((deref_mut)), UNKNOWN),
            (
                quote!(= "x"),
                "`#[newtype]` takes flags in parentheses: `#[newtype(deref, display)]`",
            ),
            (
                quote!((display = "{name}")),
                "`{name}` names no field of `S`",
            ),
            (quote!((display = "{} {}")), "`{}` names no field of `S`"),
        ] {
            assert_eq!(errors(quote! { #[newtype #flags] struct S(u8); }), [msg]);
        }
    }

    #[test]
    fn the_validator_must_be_a_sibling_function_returning_a_unit_result() {
        assert_eq!(
            errors(quote! {
                fn unit(n: &u8) {}
                fn alias(n: &u8) -> io::Result<()> { Ok(()) }
                fn value(n: &u8) -> Result<u8, String> { Ok(*n) }
                #[newtype(validate = missing)] struct A(u8);
                #[newtype(validate = m::check)] struct B(u8);
                #[newtype(validate = unit)] struct C(u8);
                #[newtype(validate = alias)] struct D(u8);
                #[newtype(validate = value)] struct E(u8);
                mod m { fn check(n: &u8) -> Result<(), ()> { Ok(()) } }
            }),
            [
                "`validate` needs a function declared next to the struct, as in \
                 `validate = check`, so `new` can name its error type",
                "`validate` needs a function declared next to the struct, as in \
                 `validate = check`, so `new` can name its error type",
                "a `validate` function returns `Result<(), E>`",
                "a `validate` function returns `Result<(), E>`",
                "a `validate` function returns `Result<(), E>`",
            ]
        );
    }

    #[test]
    fn operators_with_validate_and_a_repeated_attribute_are_reported_together() {
        assert_eq!(
            errors(quote! {
                fn check(n: &u8) -> Result<(), String> { Ok(()) }
                #[newtype(add, deref, rem, validate = check)]
                #[newtype]
                struct Small(u8);
            }),
            [
                "`#[newtype]` is given twice",
                "`add` would build a `Small` that `validate` never checked; operators cannot be \
                 combined with `validate`",
                "`rem` would build a `Small` that `validate` never checked; operators cannot be \
                 combined with `validate`",
            ]
        );
    }
}
