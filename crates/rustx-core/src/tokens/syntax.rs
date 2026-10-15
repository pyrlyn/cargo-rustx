//! Small helpers over `syn` values shared by token and AST passes: error accumulation, bare
//! type names, introduced names and pass markers.
//!
//! Separate from the token-tree helpers because they work on parsed values, yet kept in this
//! module so every pass has one place to look for shared helpers.

use proc_macro2::{Ident, Span};
use syn::{Attribute, Type};

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
    fn only_reserved_names_are_markers() {
        let attrs =
            syn::parse_str::<syn::ItemFn>("#[__rustx_default(1)] #[rustx] #[cfg(x)] fn f() {}")
                .unwrap()
                .attrs;
        let markers: Vec<bool> = attrs.iter().map(is_marker).collect();
        assert_eq!(markers, [true, false, false]);
    }
}
