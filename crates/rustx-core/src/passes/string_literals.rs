//! String literals into `String` (problem 15, `15-string-params.md`; `into T` parameters are
//! `into_params`).
//!
//! Always on, unlike `#[widen]` (ARCHITECTURE.md, section g): a string literal where the input
//! writes the type `String` is a type error in plain Rust, so converting it there cannot change
//! what valid code means. A literal, `Some(literal)`, or the literals of `vec![..]`, going into a
//! place written `String`, `Option<String>` or `Vec<String>` (see `typed_positions`; struct
//! literal fields count here) becomes `String::from`:
//!
//! ```text
//! Contact { name: "Ann", email: Some("ann@example.com"), tags: vec!["work", x] }
//! → Contact {
//!     name: ::std::string::String::from("Ann"),
//!     email: Some(::std::string::String::from("ann@example.com")),
//!     tags: vec![::std::string::String::from("work"), x],
//! }
//! ```
//!
//! The conversion names `String` rather than calling `.into()`, so a place guessed wrong (a
//! same-named struct elsewhere whose field is `&str`) fails to compile instead of converting into
//! another type. Byte strings, values that are not literals, and `const` and `static` items
//! (`String::from` is not `const`) stay as written.

use std::mem;

use quote::ToTokens;
use syn::spanned::Spanned;
use syn::{Expr, Type};

use super::macro_args::MacroArgs;
use super::typed_positions::{Convert, walk_file};
use crate::tokens::syntax::{is_str_literal, is_string, std_argument, std_type};

// Every AST pass has this signature; this one has nothing to report.
#[expect(clippy::unnecessary_wraps)]
pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    walk_file(file, &mut Strings(from_literal));
    Ok(())
}

/// Defaults are still typed here, before their parameter becomes an Option.
pub(super) fn convert_value(target: &Type, value: &mut Expr) {
    Strings(from_literal).convert(target, value, None);
}

/// The places where a literal becomes a `String`, and what happens at each: `.0` gets every
/// expression a literal written there would be converted at, and returns whether it changed it.
/// Shared with the reverse rule (`sugar/string_literals.rs`), so both directions agree on the
/// places.
pub(crate) struct Strings<F>(pub(crate) F);

impl<F: FnMut(&mut Expr) -> bool> Convert for Strings<F> {
    const STRUCT_FIELDS: bool = true;
    // `String += &str` is valid Rust, and `String += String` is not.
    const COMPOUND_ASSIGNMENTS: bool = false;

    /// The value itself for `String`, the argument of `Some(..)` for `Option<String>`, the
    /// elements of `vec![..]` (or the value of `vec![x; n]`) for `Vec<String>`.
    fn convert(&mut self, target: &Type, value: &mut Expr, _source: Option<&Type>) {
        let holds_string = |module, name| std_argument(target, module, name).is_some_and(is_string);
        if is_string(target) {
            (self.0)(value);
        } else if let Some(inner) = std_argument(target, "option", "Option") {
            if let Expr::Call(call) = value
                && let Expr::Path(func) = &*call.func
                && func.qself.is_none()
                && some_constructor(&func.path)
                && call.args.len() == 1
            {
                for arg in &mut call.args {
                    self.convert(inner, arg, None);
                }
            }
        } else if holds_string("vec", "Vec") {
            vec_elements(value, &mut self.0);
        }
    }
}

/// Defaulted calls use the qualified constructor; ordinary code may import Some.
fn some_constructor(path: &syn::Path) -> bool {
    let mut parent = path.clone();
    if parent
        .segments
        .pop()
        .is_none_or(|last| last.ident != "Some")
    {
        return false;
    }
    parent.segments.is_empty()
        || std_type(
            &Type::Path(syn::TypePath {
                attrs: Vec::new(),
                qself: None,
                path: parent,
            }),
            "option",
            "Option",
        )
        .is_some()
}

/// `"text"` → `::std::string::String::from("text")`, at the literal's span.
fn from_literal(value: &mut Expr) -> bool {
    if !is_str_literal(value) {
        return false;
    }
    let literal = mem::replace(value, Expr::PLACEHOLDER);
    *value = syn::parse_quote_spanned! {literal.span()=>
        ::std::string::String::from(#literal)
    };
    true
}

/// Calls `at` on the elements of `vec![a, b]`, or the value of `vec![a; n]`. Other elements keep
/// their own type: in plain Rust they already had to be `String`. The body is re-emitted only
/// when `at` changed an element.
fn vec_elements(value: &mut Expr, at: &mut impl FnMut(&mut Expr) -> bool) {
    let Expr::Macro(mac) = value else {
        return;
    };
    if !mac.mac.path.is_ident("vec") {
        return;
    }
    let Ok(mut args) = mac.mac.parse_body::<MacroArgs>() else {
        return;
    };
    let elements: Vec<&mut Expr> = match &mut args {
        MacroArgs::List(list) => list.iter_mut().collect(),
        MacroArgs::Repeat(element, ..) => vec![&mut **element],
    };
    // Every element is visited, so no short-circuiting `any`.
    let changed = elements
        .into_iter()
        .fold(false, |changed, element| at(element) | changed);
    if changed {
        mac.mac.tokens = args.into_token_stream();
    }
}

#[cfg(test)]
mod tests {
    use proc_macro2::TokenStream;
    use quote::{ToTokens, quote};

    fn expanded(sugar: TokenStream) -> String {
        crate::try_expand(sugar)
            .map_err(|err| err.to_string())
            .unwrap()
            .into_token_stream()
            .to_string()
    }

    fn plain(rust: TokenStream) -> String {
        syn::parse2::<syn::File>(rust)
            .unwrap()
            .into_token_stream()
            .to_string()
    }

    #[test]
    fn literals_into_written_string_types_are_converted_in_every_position() {
        let from = quote!(::std::string::String::from);
        assert_eq!(
            expanded(quote! {
                struct Contact { name: String, email: Option<String>, tags: Vec<String> }
                struct Pair(String, std::string::String);
                impl Contact {
                    fn rename(&mut self, name: String) { self.name = "anon"; }
                    fn new() -> Self { Self { name: "Ann", email: None, tags: vec![] } }
                }
                fn put(name: String, tags: Vec<String>) {}
                fn f(c: &mut Contact, mut p: Pair) {
                    let a: String = "a";
                    let mut b: Option<String> = Some(r"b");
                    let v: Vec<String> = vec!["x", "y",];
                    let w: Vec<String> = vec!["z"; 3];
                    b = Some("c");
                    c.email = Some("d");
                    p.1 = "e";
                    c.rename("f");
                    put("g", vec!["g"]);
                    let k = Contact { name: "h", email: Some("i"), tags: vec!["j", a] };
                    let m = |s: String| { let t: String = "l"; };
                }
            }),
            plain(quote! {
                struct Contact { name: String, email: Option<String>, tags: Vec<String> }
                struct Pair(String, std::string::String);
                impl Contact {
                    fn rename(&mut self, name: String) { self.name = #from("anon"); }
                    fn new() -> Self { Self { name: #from("Ann"), email: None, tags: vec![] } }
                }
                fn put(name: String, tags: Vec<String>) {}
                fn f(c: &mut Contact, mut p: Pair) {
                    let a: String = #from("a");
                    let mut b: Option<String> = Some(#from(r"b"));
                    let v: Vec<String> = vec![#from("x"), #from("y"),];
                    let w: Vec<String> = vec![#from("z"); 3];
                    b = Some(#from("c"));
                    c.email = Some(#from("d"));
                    p.1 = #from("e");
                    c.rename("f");
                    put(#from("g"), vec![#from("g")]);
                    let k = Contact {
                        name: #from("h"),
                        email: Some(#from("i")),
                        tags: vec![#from("j"), a]
                    };
                    let m = |s: String| { let t: String = #from("l"); };
                }
            })
        );
    }

    #[test]
    fn str_typed_and_untyped_positions_are_unchanged() {
        let input = quote! {
            struct Label { text: &'static str, alt: Option<&'static str> }
            fn put(s: &str) {}
            fn f() {
                let a: &str = "a";
                let b = "b";
                let c: Option<&str> = Some("c");
                let d: Vec<&str> = vec!["d"];
                let e = Label { text: "e", alt: Some("f") };
                let g = Other { name: "g" };
                put("h");
                let mut map = std::collections::HashMap::new();
                map.insert("i", 1);
            }
        };
        assert_eq!(expanded(input.clone()), plain(input));
    }

    #[test]
    fn byte_strings_values_that_are_already_strings_and_appends_are_unchanged() {
        let input = quote! {
            struct S { name: String, tags: Vec<String> }
            fn f(name: String) {
                let a: String = b"a";
                let b: String = c"b";
                let c: String = "c".to_string();
                let mut s: String = name.clone();
                s += "more";
                let d: String = name;
                let e: Option<String> = None;
                let g: Vec<String> = vec![name.clone(), String::new()];
                let h: Option<String> = Ok("h");
                let s = S { name, tags: Vec::new() };
            }
        };
        assert_eq!(expanded(input.clone()), plain(input));
    }

    #[test]
    fn consts_statics_and_types_that_only_look_like_string_are_unchanged() {
        let input = quote! {
            struct S { name: String }
            const C: String = "c";
            static D: S = S { name: "d" };
            fn f() {
                let a: my::String = "a";
                let b: Option<String, u8> = Some("b");
                let c: Vec<String> = my::vec!["c"];
                let d: Vec<String> = vec![];
            }
        };
        assert_eq!(expanded(input.clone()), plain(input));
    }
}
