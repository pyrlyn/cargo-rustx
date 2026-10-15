//! Flag lists of the derive-like attributes: `#[data(setters, builder)]`,
//! `#[newtype(deref, validate = check)]`.
//!
//! Shared so every such attribute rejects `#[name = ..]`, repeated flags and stray values with
//! the same words.

use std::collections::HashSet;

use quote::ToTokens;
use syn::meta::ParseNestedMeta;
use syn::{Attribute, Meta, Token};

/// Calls `flag` on each flag of `attr`; `usage` is shown when the flags are not in parentheses.
/// `flag` reports an unknown flag itself, and returns whether it read a value (`f = v`); any
/// other flag must stand alone.
pub(super) fn parse_flags(
    attr: &Attribute,
    usage: &str,
    mut flag: impl FnMut(&ParseNestedMeta) -> syn::Result<bool>,
) -> syn::Result<()> {
    let name = attr.path().to_token_stream().to_string();
    match &attr.meta {
        Meta::Path(_) => return Ok(()),
        Meta::NameValue(meta) => {
            let msg = format!("`#[{name}]` takes flags in parentheses: `{usage}`");
            return Err(syn::Error::new_spanned(&meta.value, msg));
        }
        Meta::List(_) => {}
    }
    let mut seen = HashSet::new();
    attr.parse_nested_meta(|meta| {
        let read_value = flag(&meta)?;
        let written = meta.path.to_token_stream().to_string();
        if !seen.insert(written.clone()) {
            return Err(meta.error(format!("this `#[{name}]` flag is given twice")));
        }
        if !read_value && !meta.input.is_empty() && !meta.input.peek(Token![,]) {
            let msg = format!("the `#[{name}]` flag `{written}` takes no value");
            return Err(meta.error(msg));
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    /// The flags of `attr` as `name` or `name=value`; only `a` and `v` are known.
    fn flags(attr: &Attribute) -> syn::Result<Vec<String>> {
        let mut out = Vec::new();
        parse_flags(attr, "#[t(a, v = 1)]", |meta| {
            let name = meta.path.to_token_stream().to_string();
            match name.as_str() {
                "a" => out.push(name),
                "v" => {
                    let value: syn::Expr = meta.value()?.parse()?;
                    out.push(format!("v={}", value.to_token_stream()));
                    return Ok(true);
                }
                _ => return Err(meta.error("unknown")),
            }
            Ok(false)
        })?;
        Ok(out)
    }

    #[test]
    fn flags_are_read_in_order_and_a_bare_attribute_has_none() {
        assert_eq!(flags(&parse_quote!(#[t(a, v = 1,)])).unwrap(), ["a", "v=1"]);
        assert_eq!(flags(&parse_quote!(#[t])).unwrap(), Vec::<String>::new());
    }

    #[test]
    fn misplaced_values_and_repeated_flags_are_errors() {
        for (attr, msg) in [
            (
                parse_quote!(#[t = 1]),
                "`#[t]` takes flags in parentheses: `#[t(a, v = 1)]`",
            ),
            (parse_quote!(#[t(a, a)]), "this `#[t]` flag is given twice"),
            (
                parse_quote!(#[t(a = 1)]),
                "the `#[t]` flag `a` takes no value",
            ),
            (parse_quote!(#[t(b)]), "unknown"),
        ] {
            assert_eq!(flags(&attr).unwrap_err().to_string(), msg);
        }
    }
}
