//! Attribute macros: the derive-like sugar on items that are already valid Rust, outside `sugar!`.
//!
//! rustc parses an attribute macro's item before the macro runs, so only sugar that is valid
//! Rust syntax works here (ARCHITECTURE.md, section d); token passes have nothing to do and are
//! skipped. The item is rebuilt into a [`syn::File`] and the AST passes behind the attribute run
//! on it, exactly as inside `sugar!`.
//!
//! The macro sees only its item, so what the passes would find next to it is unknown: a trait
//! `#[delegate]` should forward and the impl `except(..)` adds to, or a type the builder name of
//! `#[data(builder)]` collides with. `#[sugared]` on an inline module sees everything inside it.

use proc_macro2::{Delimiter, Group, Ident, Span, TokenStream};
use quote::ToTokens;
use syn::{Attribute, Item};

use crate::passes;
use crate::tokens::syntax::combine;

/// An attribute macro of cargo-rustx: the sugar it applies to the item it sits on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AttributeMacro {
    /// `#[data]` / `#[data(setters, with, builder)]` on a struct with named fields.
    Data,
    /// `#[display]` on an enum whose variants carry `#[display("..")]`, with `#[from]` on the
    /// field of a variant that converts from it.
    Display,
    /// `#[delegate]` on a struct whose fields carry `#[delegate(Trait, ..)]`.
    Delegate,
    /// `#[newtype(deref, display, ..)]` on a one-field tuple struct.
    Newtype,
    /// `#[sugared]` on any item: every attribute-shaped sugar inside it.
    Sugared,
}

impl AttributeMacro {
    fn name(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Display => "display",
            Self::Delegate => "delegate",
            Self::Newtype => "newtype",
            Self::Sugared => "sugared",
        }
    }

    /// Whether the pass reads the macro's own attribute, so it goes back onto the item with its
    /// arguments; the others take no arguments.
    fn reads_itself(self) -> bool {
        matches!(self, Self::Data | Self::Newtype)
    }

    /// Why the macro does not fit `item`, if it does not. A macro that would generate nothing is
    /// reported too, so a forgotten field or variant attribute does not go unnoticed.
    fn misuse(self, item: &Item) -> Option<&'static str> {
        match (self, item) {
            (Self::Sugared, _)
            | (Self::Data | Self::Newtype, Item::Struct(_) | Item::Enum(_) | Item::Union(_)) => {
                None
            }
            (Self::Display, Item::Enum(item))
                if item.variants.iter().any(|v| has_attr(&v.attrs, "display")) =>
            {
                None
            }
            (Self::Delegate, Item::Struct(item))
                if item.fields.iter().any(|f| has_attr(&f.attrs, "delegate")) =>
            {
                None
            }
            (Self::Data, _) => Some("`#[data]` needs a struct with named fields"),
            (Self::Newtype, _) => Some("`#[newtype]` needs a tuple struct with one field"),
            (Self::Display, _) => Some(
                "`#[display]` needs an enum whose variants carry `#[display(\"..\")]`; \
                 for `#[from]` alone, use `#[sugared]`",
            ),
            (Self::Delegate, _) => {
                Some("`#[delegate]` needs a struct with `#[delegate(Trait)]` on a field")
            }
        }
    }
}

/// Expands an attribute macro on `item`: the item with the sugar applied, followed by every
/// desugaring error as a `compile_error!`.
///
/// The item is kept even when there are errors, so code using it does not add a cascade of
/// unrelated errors to the ones reported.
#[must_use]
pub fn expand_attribute(kind: AttributeMacro, args: TokenStream, item: TokenStream) -> TokenStream {
    let (mut out, errors) = lower(kind, args, item);
    if let Some(errors) = errors {
        out.extend(errors.to_compile_error());
    }
    out
}

fn lower(
    kind: AttributeMacro,
    args: TokenStream,
    item: TokenStream,
) -> (TokenStream, Option<syn::Error>) {
    let mut errors = None;
    // The call site is the user's attribute, so errors about the macro point there.
    let name = Ident::new(kind.name(), Span::call_site());
    if !kind.reads_itself() && !args.is_empty() {
        let msg = format!("`#[{name}]` takes no arguments");
        combine(&mut errors, syn::Error::new_spanned(args.clone(), msg));
    }
    let mut file: syn::File = match syn::parse2(item.clone()) {
        Ok(file) => file,
        Err(err) => {
            combine(&mut errors, err);
            return (item, errors);
        }
    };
    if let Some(first) = file.items.first_mut() {
        match kind.misuse(first) {
            Some(msg) => combine(&mut errors, syn::Error::new(Span::call_site(), msg)),
            None if kind.reads_itself() => {
                let args = Group::new(Delimiter::Parenthesis, args);
                if let Some(attrs) = type_attrs(first) {
                    attrs.insert(0, syn::parse_quote!(#[#name #args]));
                }
            }
            None => {}
        }
    }
    // The passes run even after a misuse, so they take off the attributes rustc would report as
    // unknown.
    if let Err(found) = passes::rewrite_attribute(kind, &mut file) {
        for err in found {
            let err = match kind {
                AttributeMacro::Sugared => err,
                _ => alone(err),
            };
            combine(&mut errors, err);
        }
    }
    (file.into_token_stream(), errors)
}

/// The attributes of a type declaration, where a macro that reads itself puts itself back.
fn type_attrs(item: &mut Item) -> Option<&mut Vec<Attribute>> {
    match item {
        Item::Struct(item) => Some(&mut item.attrs),
        Item::Enum(item) => Some(&mut item.attrs),
        Item::Union(item) => Some(&mut item.attrs),
        _ => None,
    }
}

fn has_attr(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| attr.path().is_ident(name))
}

/// Rewords a pass error that tells the user to look next to the item, which a macro on the item
/// alone cannot see.
///
/// Matched by the passes' own message constants, because the pass owns that wording for
/// `sugar!`, where it is right.
fn alone(err: syn::Error) -> syn::Error {
    let msg = err.to_string();
    let reworded = if msg.starts_with(passes::UNKNOWN_TRAIT) {
        "`#[delegate]` on a struct knows only the built-in std traits; list the methods to \
         forward, as in `Shape { fn area(&self) -> f64; }`, or put `#[sugared]` on a module that \
         declares the trait"
    } else if msg.starts_with(passes::EXCEPT_WITHOUT_IMPL) {
        "`#[delegate]` on a struct cannot see the hand-written impl `except(..)` adds to; put \
         `#[sugared]` on a module that holds both"
    } else if msg == passes::VALIDATOR_NOT_FOUND {
        "`#[newtype]` on a struct cannot see the `validate` function next to it; put `#[sugared]` \
         on a module that declares both"
    } else {
        return err;
    };
    syn::Error::new(err.span(), reworded)
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    fn expanded(kind: AttributeMacro, args: TokenStream, item: TokenStream) -> String {
        let (out, errors) = lower(kind, args, item);
        assert!(errors.is_none(), "{errors:?}");
        out.to_string()
    }

    /// The error messages, and the item that is still emitted next to them.
    fn errors(kind: AttributeMacro, args: TokenStream, item: TokenStream) -> (Vec<String>, String) {
        let (out, errors) = lower(kind, args, item);
        let messages = errors.into_iter().flatten().map(|err| err.to_string());
        (messages.collect(), out.to_string())
    }

    fn printed(plain: TokenStream) -> String {
        syn::parse2::<syn::File>(plain)
            .unwrap()
            .into_token_stream()
            .to_string()
    }

    #[test]
    fn data_runs_with_its_flags_and_leaves_no_attribute() {
        let out = expanded(
            AttributeMacro::Data,
            quote!(builder),
            quote! { pub struct User { name: String, #[default(18)] age: u8 } },
        );
        for part in [
            "pub struct User { name : String , age : u8 }",
            "pub fn new (name : String) -> Self",
            "pub fn builder ()",
            "struct UserBuilder",
        ] {
            assert!(out.contains(part), "{part} in {out}");
        }
        assert!(!out.contains("# [data"), "{out}");
    }

    #[test]
    fn display_turns_from_fields_into_from_impls_and_writes_display() {
        let out = expanded(
            AttributeMacro::Display,
            TokenStream::new(),
            quote! {
                #[derive(Debug)]
                enum E { #[display("io: {0}")] Io(#[from] io::Error), #[display("gone")] Gone }
            },
        );
        for part in [
            "enum E { Io (io :: Error) , Gone }",
            ":: core :: convert :: From < io :: Error > for E",
            ":: core :: fmt :: Display for E",
            ":: std :: error :: Error for E",
        ] {
            assert!(out.contains(part), "{part} in {out}");
        }
    }

    #[test]
    fn delegate_forwards_a_std_trait_and_a_listed_one() {
        let out = expanded(
            AttributeMacro::Delegate,
            TokenStream::new(),
            quote! {
                struct W { #[delegate(Write)] w: File, #[delegate(Len { fn len(&self) -> usize; })] v: V }
            },
        );
        for part in [
            "struct W { w : File , v : V }",
            "impl :: std :: io :: Write for W",
            "impl Len for W",
        ] {
            assert!(out.contains(part), "{part} in {out}");
        }
    }

    #[test]
    fn newtype_runs_with_its_flags_and_leaves_no_attribute() {
        let out = expanded(
            AttributeMacro::Newtype,
            quote!(deref, from_str),
            quote! { pub struct Email(String); },
        );
        for part in [
            "pub struct Email (String) ;",
            "pub fn new",
            "Deref for Email",
            "FromStr for Email",
        ] {
            assert!(out.contains(part), "{part} in {out}");
        }
        assert!(!out.contains("# [newtype"), "{out}");
    }

    #[test]
    fn newtype_validate_needs_the_module_and_says_so() {
        let (messages, out) = errors(
            AttributeMacro::Newtype,
            quote!(validate = check),
            quote! { pub struct Email(String); },
        );
        assert_eq!(
            messages,
            [
                "`#[newtype]` on a struct cannot see the `validate` function next to it; put \
                 `#[sugared]` on a module that declares both"
            ]
        );
        assert_eq!(out, printed(quote! { pub struct Email(String); }));
        let out = expanded(
            AttributeMacro::Sugared,
            TokenStream::new(),
            quote! {
                mod m {
                    #[newtype(validate = check)] pub struct Email(String);
                    fn check(s: &String) -> Result<(), &'static str> { Ok(()) }
                }
            },
        );
        assert!(out.contains("Result < Self , & 'static str >"), "{out}");
    }

    #[test]
    fn sugared_runs_every_ast_pass_inside_the_item() {
        let out = expanded(
            AttributeMacro::Sugared,
            TokenStream::new(),
            quote! {
                mod m {
                    #[borrows] #[data] pub struct Line { text: &str }
                    impl Line { fn first(&self) -> &str { self.text } }
                    enum E { A(#[from] u8) }
                }
            },
        );
        for part in [
            "pub struct Line < '__rustx_borrows > { text : & '__rustx_borrows str }",
            "impl < '__rustx_borrows > Line < '__rustx_borrows >",
            "From < u8 > for E",
        ] {
            assert!(out.contains(part), "{part} in {out}");
        }
    }

    #[test]
    fn valid_rust_without_sugar_is_unchanged_by_sugared() {
        let plain = quote! { impl S { fn f(&self) -> u8 { 1 } } };
        assert_eq!(
            expanded(AttributeMacro::Sugared, TokenStream::new(), plain.clone()),
            printed(plain)
        );
    }

    #[test]
    fn arguments_where_none_are_taken_are_reported_and_the_sugar_still_applies() {
        let (messages, out) = errors(
            AttributeMacro::Sugared,
            quote!(fast),
            quote! { enum E { A(#[from] u8) } },
        );
        assert_eq!(messages, ["`#[sugared]` takes no arguments"]);
        assert!(out.contains("From < u8 >"), "{out}");
    }

    #[test]
    fn a_misplaced_or_idle_macro_is_an_error_and_its_sugar_still_applies() {
        for (kind, item, msg) in [
            (
                AttributeMacro::Data,
                quote!(
                    fn f() {}
                ),
                "`#[data]` needs a struct with named fields",
            ),
            (
                AttributeMacro::Display,
                quote!(
                    enum E {
                        A(#[from] u8),
                    }
                ),
                "`#[display]` needs an enum whose variants carry `#[display(\"..\")]`; \
                 for `#[from]` alone, use `#[sugared]`",
            ),
            (
                AttributeMacro::Delegate,
                quote!(
                    enum E {
                        A(#[delegate(Read)] File),
                    }
                ),
                "`#[delegate]` needs a struct with `#[delegate(Trait)]` on a field",
            ),
            (
                AttributeMacro::Delegate,
                quote!(
                    struct S {
                        a: u8,
                    }
                ),
                "`#[delegate]` needs a struct with `#[delegate(Trait)]` on a field",
            ),
        ] {
            let (messages, out) = errors(kind, TokenStream::new(), item);
            assert_eq!(messages[0], msg);
            assert!(
                !out.contains("# [from]") && !out.contains("# [delegate"),
                "{out}"
            );
        }
    }

    #[test]
    fn pass_errors_are_combined_and_the_item_loses_its_markers() {
        let (messages, out) = errors(
            AttributeMacro::Data,
            quote!(getters),
            quote! { struct S { #[default] a: u8 } },
        );
        assert_eq!(
            messages,
            [
                "unknown `#[data]` flag; expected `setters`, `with` or `builder`",
                "`#[default]` on a `#[data]` field needs a value: `#[default(expr)]`",
            ]
        );
        assert_eq!(out, printed(quote! { struct S { a: u8 } }));
    }

    #[test]
    fn delegate_errors_that_need_the_rest_of_the_input_say_so() {
        let (messages, out) = errors(
            AttributeMacro::Delegate,
            TokenStream::new(),
            quote! {
                struct S { #[delegate(Shape)] a: Sq, #[delegate(Write, except(write))] w: File }
            },
        );
        assert_eq!(messages.len(), 2, "{messages:?}");
        assert!(
            messages[0].starts_with("`#[delegate]` on a struct knows only the built-in std traits"),
            "{messages:?}"
        );
        assert!(
            messages[1].starts_with("`#[delegate]` on a struct cannot see the hand-written impl"),
            "{messages:?}"
        );
        assert_eq!(out, printed(quote! { struct S { a: Sq, w: File } }));
    }

    #[test]
    fn other_delegate_errors_are_kept() {
        let (messages, _) = errors(
            AttributeMacro::Delegate,
            TokenStream::new(),
            quote! { struct S { #[delegate(Write)] a: File, #[delegate(Write)] b: File } },
        );
        assert_eq!(
            messages,
            ["`Write` is already delegated to field `a` of `S`; a trait has one impl"]
        );
    }

    #[test]
    fn the_expansion_carries_the_item_and_the_compile_error() {
        let out = expand_attribute(
            AttributeMacro::Data,
            TokenStream::new(),
            quote!(
                struct T(u8);
            ),
        )
        .to_string();
        assert!(out.starts_with("struct T (u8) ;"), "{out}");
        assert!(out.contains("compile_error"), "{out}");
    }
}
