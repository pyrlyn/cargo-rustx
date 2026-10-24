//! Small helpers over `syn` values shared by token and AST passes: error accumulation, bare
//! type names, introduced names and pass markers.
//!
//! Separate from the token-tree helpers because they work on parsed values, yet kept in this
//! module so every pass has one place to look for shared helpers.

use proc_macro2::{Ident, Span};
use syn::punctuated::Punctuated;
use syn::{Attribute, Expr, ExprLit, GenericArgument, Lit, Path, PathArguments, Token, Type};

/// Prefix of every name the passes generate; reserved, so user names cannot collide with them.
pub(crate) const RESERVED_PREFIX: &str = "__rustx_";

/// Adds `err` to `errors`, so one run reports every error instead of only the first.
pub(crate) fn combine(errors: &mut Option<syn::Error>, err: syn::Error) {
    match errors {
        Some(all) => all.combine(err),
        None => *errors = Some(err),
    }
}

/// The name of a type that is a bare identifier (`T`), not a path, a qualified path or a
/// compound type.
pub(crate) fn bare_name(ty: &Type) -> Option<&Ident> {
    match ty {
        Type::Path(path) if path.qself.is_none() => path.path.get_ident(),
        _ => None,
    }
}

/// The span of a name a pass introduces: resolved like macro output, so it cannot clash with a
/// user name, but reported at the user's token.
pub(crate) fn introduced_span(user: Span) -> Span {
    Span::mixed_site().located_at(user)
}

/// Removes the attribute called `name`, the marker a token pass left for its AST pass.
pub(crate) fn take_attr(attrs: &mut Vec<Attribute>, name: &str) -> Option<Attribute> {
    let at = attrs.iter().position(|attr| attr.path().is_ident(name))?;
    Some(attrs.remove(at))
}

/// Whether `attr` is some pass's marker rather than a Rust attribute, so copies of a parameter's
/// attributes can leave markers of passes that have not run yet behind.
/// The paths of an item's `#[derive(..)]` lists, in order. A list that does not parse is
/// skipped: rustc reports it on the output, where it stays.
pub(crate) fn derived_paths(attrs: &[Attribute]) -> impl Iterator<Item = Path> + '_ {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("derive"))
        .filter_map(|attr| {
            attr.parse_args_with(Punctuated::<Path, Token![,]>::parse_terminated)
                .ok()
        })
        .flatten()
}

/// The generic arguments of `ty` when it is written as the std type `name` of `module`: bare,
/// or through `std` or `alloc`; `Option` also accepts `core::option`. A macro sees only the written path, so a
/// renamed import or an alias is not recognised.
pub(crate) fn std_type<'a>(ty: &'a Type, module: &str, name: &str) -> Option<&'a PathArguments> {
    let Type::Path(path) = ty else {
        return None;
    };
    let mut segments = path.path.segments.iter();
    let last = segments.next_back()?;
    let prefix: Vec<String> = segments
        .map(|s| s.arguments.is_none().then(|| s.ident.to_string()))
        .collect::<Option<_>>()?;
    let in_std = match &prefix[..] {
        [] => true,
        [root, inner] => {
            (root == "std" || root == "alloc" || (root == "core" && module == "option"))
                && inner == module
        }
        _ => false,
    };
    (path.qself.is_none() && in_std && last.ident == name).then_some(&last.arguments)
}

/// `T` of the std type `name` of `module` written with one type argument (`Vec<T>`,
/// `std::option::Option<T>`), recognised as `std_type` does.
pub(crate) fn std_argument<'a>(ty: &'a Type, module: &str, name: &str) -> Option<&'a Type> {
    let PathArguments::AngleBracketed(args) = std_type(ty, module, name)? else {
        return None;
    };
    match (args.args.first(), args.args.len()) {
        (Some(GenericArgument::Type(argument)), 1) => Some(argument),
        _ => None,
    }
}

/// `String`, `std::string::String` or `alloc::string::String`.
pub(crate) fn is_string(ty: &Type) -> bool {
    std_type(ty, "string", "String").is_some_and(PathArguments::is_none)
}

/// A string literal, raw or not; byte and C strings are not `str`.
pub(crate) fn is_str_literal(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Lit(ExprLit {
            lit: Lit::Str(_),
            ..
        })
    )
}

pub(crate) fn is_marker(attr: &Attribute) -> bool {
    attr.path()
        .get_ident()
        .is_some_and(|name| name.to_string().starts_with(RESERVED_PREFIX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn combined_errors_keep_every_message_in_order() {
        let mut errors = None;
        combine(&mut errors, syn::Error::new(Span::call_site(), "first"));
        combine(&mut errors, syn::Error::new(Span::call_site(), "second"));
        let messages: Vec<String> = errors
            .into_iter()
            .flatten()
            .map(|err| err.to_string())
            .collect();
        assert_eq!(messages, ["first", "second"]);
    }

    #[test]
    fn only_a_plain_identifier_type_has_a_bare_name() {
        let name = |source: &str| {
            bare_name(&syn::parse_str::<Type>(source).unwrap()).map(ToString::to_string)
        };
        assert_eq!(name("T").as_deref(), Some("T"));
        for not_bare in ["a::T", "::T", "Vec<T>", "&T", "<T as Tr>::A", "(T)"] {
            assert_eq!(name(not_bare), None, "{not_bare}");
        }
    }

    #[test]
    fn take_attr_removes_only_the_named_attribute() {
        let mut attrs =
            syn::parse_str::<syn::ItemFn>("#[inline] #[__rustx_into] #[cfg(unix)] fn f() {}")
                .unwrap()
                .attrs;
        let taken = take_attr(&mut attrs, "__rustx_into").unwrap();
        assert!(taken.path().is_ident("__rustx_into"));
        assert_eq!(attrs.len(), 2);
        assert!(take_attr(&mut attrs, "__rustx_into").is_none());
    }

    #[test]
    fn derived_paths_come_from_every_parsable_derive_list_in_order() {
        let item = syn::parse_str::<syn::ItemStruct>(
            "#[derive(Debug, std::hash::Hash)] #[cfg(x)] #[derive(= bad)] #[derive(Clone,)] struct S;",
        )
        .unwrap();
        let paths: Vec<String> = derived_paths(&item.attrs)
            .map(|path| quote::ToTokens::to_token_stream(&path).to_string())
            .collect();
        assert_eq!(paths, ["Debug", "std :: hash :: Hash", "Clone"]);
    }

    #[test]
    fn std_types_are_recognised_by_their_written_path() {
        let ty = |source: &str| syn::parse_str::<Type>(source).unwrap();
        for string in ["String", "std::string::String", "alloc::string::String"] {
            assert!(is_string(&ty(string)), "{string}");
        }
        for other in [
            "my::String",
            "std::String",
            "core::string::String",
            "String<u8>",
            "<T as X>::String",
            "&String",
        ] {
            assert!(!is_string(&ty(other)), "{other}");
        }
        assert!(std_type(&ty("std::vec::Vec<u8>"), "vec", "Vec").is_some());
        assert!(std_type(&ty("core::vec::Vec<u8>"), "vec", "Vec").is_none());
        let argument = |source: &str| {
            std_argument(&ty(source), "option", "Option")
                .map(|arg| quote::ToTokens::to_token_stream(arg).to_string())
        };
        assert_eq!(argument("std::option::Option<u8>").as_deref(), Some("u8"));
        assert_eq!(argument("core::option::Option<u8>").as_deref(), Some("u8"));
        for other in ["Option", "Option<'a>", "Option<u8, u8>", "Option<N = u8>"] {
            assert_eq!(argument(other), None, "{other}");
        }
    }

    #[test]
    fn only_reserved_names_are_markers() {
        let attrs =
            syn::parse_str::<syn::ItemFn>("#[__rustx_default(1)] #[rustx] #[cfg(x)] fn f() {}")
                .unwrap()
                .attrs;
        let markers: Vec<bool> = attrs.iter().map(is_marker).collect();
        assert_eq!(markers, [true, false, false]);
    }
}
