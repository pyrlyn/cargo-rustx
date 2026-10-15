//! Small helpers over `syn` values shared by token and AST passes: error accumulation and bare
//! type names.
//!
//! Separate from the token-tree helpers because they work on parsed values, yet kept in this
//! module so every pass has one place to look for shared helpers.

use proc_macro2::Ident;
use syn::Type;

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

#[cfg(test)]
mod tests {
    use super::*;
    use proc_macro2::Span;

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
}
