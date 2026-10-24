//! Code generation: one module per pattern.

pub(crate) mod bind;
mod builder;
mod config;
mod convert;
mod enum_str;
mod error;
mod getters;
mod new;
mod newtype;
mod registry;
mod setters;

use proc_macro2::TokenStream;
use syn::{Data, DeriveInput, Field, Fields, Type, Variant};

use crate::parse::{self, FieldConfig, Pattern};

/// Dispatch a validated pattern to its generator.
pub(crate) fn expand(pattern: &Pattern, input: &DeriveInput) -> syn::Result<TokenStream> {
    match pattern {
        Pattern::Builder => builder::expand(input),
        Pattern::Getters => getters::expand(input),
        Pattern::Setters => setters::expand(input),
        Pattern::New => new::expand(input),
        Pattern::Error => error::expand(input),
        Pattern::EnumStr => enum_str::expand(input),
        Pattern::Convert => convert::expand(input),
        Pattern::Newtype => newtype::expand(input),
        Pattern::Registry => registry::expand(input),
        Pattern::Config { prefix } => config::expand(input, prefix),
    }
}

/// Named fields with their parsed configs (shape already validated by the rules).
pub(crate) fn named_fields(input: &DeriveInput) -> syn::Result<Vec<(&Field, FieldConfig)>> {
    match &input.data {
        Data::Struct(s) => match &s.fields {
            Fields::Named(f) => f
                .named
                .iter()
                .map(|f| Ok((f, parse::parse_field(f)?)))
                .collect(),
            _ => Err(syn::Error::new_spanned(
                &input.ident,
                "expected named fields",
            )),
        },
        _ => Err(syn::Error::new_spanned(&input.ident, "expected a struct")),
    }
}

/// Constructors and skipped builder fields must use the same default expression.
fn field_default(config: &FieldConfig) -> TokenStream {
    config.default.as_ref().map_or_else(
        || quote::quote!(::core::default::Default::default()),
        |value| quote::quote!(#value),
    )
}

/// The inner type of a one-field tuple struct.
pub(crate) fn newtype_inner(input: &DeriveInput) -> syn::Result<&Type> {
    if let Data::Struct(s) = &input.data
        && let Fields::Unnamed(f) = &s.fields
        && f.unnamed.len() == 1
    {
        return Ok(&f.unnamed[0].ty);
    }
    Err(syn::Error::new_spanned(
        &input.ident,
        "expected a one-field tuple struct",
    ))
}

/// Enum variants.
pub(crate) fn variants(input: &DeriveInput) -> syn::Result<Vec<&Variant>> {
    match &input.data {
        Data::Enum(e) => Ok(e.variants.iter().collect()),
        _ => Err(syn::Error::new_spanned(&input.ident, "expected an enum")),
    }
}

#[cfg(test)]
mod tests {
    use syn::{DeriveInput, parse_quote};

    fn expand(input: DeriveInput) -> String {
        crate::expand_derive(&input)
            .unwrap()
            .to_string()
            .replace(' ', "")
    }
    fn expand_err(input: DeriveInput) -> String {
        crate::expand_derive(&input).unwrap_err().to_string()
    }
    fn has(out: &str, needle: &str) {
        assert!(
            out.contains(&needle.replace(' ', "")),
            "missing `{needle}` in:\n{out}"
        );
    }

    #[test]
    fn builder_codegen() {
        let o = expand(parse_quote! {
            #[boil(builder)] struct Req { url: String, #[boil(default = 30)] timeout: u32, #[boil(skip)] n: u8 }
        });
        has(&o, "struct ReqBuilder");
        has(&o, "url: ::core::option::Option<String>");
        has(&o, "pub fn builder() -> ReqBuilder");
        has(&o, "pub fn url(mut self");
        has(
            &o,
            "pub fn build(self) -> ::core::result::Result<Req, ::std::string::String>",
        );
        has(&o, "unwrap_or_else(|| 30)");
        assert!(!o.contains("pubfnn("), "skipped field has no setter");
    }

    #[test]
    fn getters_codegen() {
        let o = expand(
            parse_quote! { #[boil(getters)] struct P { name: String, #[boil(skip)] secret: u8 } },
        );
        has(&o, "pub fn name(&self) -> &String { &self.name }");
        assert!(!o.contains("fnsecret"));
    }

    #[test]
    fn setters_codegen() {
        let o = expand(parse_quote! { #[boil(setters)] struct P { name: String } });
        has(
            &o,
            "pub fn set_name(&mut self, value: impl ::core::convert::Into<String>) -> &mut Self",
        );
    }

    #[test]
    fn new_codegen() {
        let o = expand(
            parse_quote! { #[boil(new)] struct P { a: u8, #[boil(default = 7)] b: u8, #[boil(skip)] c: u8 } },
        );
        has(&o, "pub fn new(a: impl ::core::convert::Into<u8>) -> Self");
        has(&o, "b: 7");
        has(&o, "c: ::core::default::Default::default()");
    }

    #[test]
    fn error_codegen() {
        let o = expand(parse_quote! {
            #[boil(error)] enum E {
                #[boil(msg = "not found: {key}")] NotFound { key: String },
                #[boil(msg = "io: {_0}", from)] Io(std::io::Error),
                #[boil(msg = "boom")] Boom,
            }
        });
        has(&o, "impl ::core::fmt::Display for E");
        has(&o, "impl ::std::error::Error for E");
        has(
            &o,
            "Self::NotFound { key } => ::core::write!(f, \"not found: {key}\")",
        );
        has(&o, "impl ::core::convert::From<std::io::Error> for E");
        has(&o, "Self::Boom => ::core::write!(f, \"boom\")");
    }

    #[test]
    fn error_requires_msg_and_single_field_from() {
        let e = expand_err(parse_quote! { #[boil(error)] enum E { A } });
        assert!(e.contains("msg"), "{e}");
        let e = expand_err(
            parse_quote! { #[boil(error)] enum E { #[boil(msg = "x", from)] A(u8, u8) } },
        );
        assert!(e.contains("from"), "{e}");
    }

    #[test]
    fn enum_str_codegen() {
        let o = expand(
            parse_quote! { #[boil(enum_str)] enum C { Red, #[boil(rename = "dark-blue")] Blue } },
        );
        has(&o, "pub const fn as_str(&self) -> &'static str");
        has(&o, "Self::Red => \"Red\"");
        has(
            &o,
            "\"dark-blue\" => ::core::result::Result::Ok(Self::Blue)",
        );
        has(&o, "impl ::core::str::FromStr for C");
        has(&o, "impl ::core::fmt::Display for C");
    }

    #[test]
    fn convert_codegen() {
        let o = expand(parse_quote! { #[boil(convert)] struct Id(u64); });
        has(&o, "impl ::core::convert::From<u64> for Id");
        has(&o, "impl ::core::convert::From<Id> for u64");
    }

    #[test]
    fn newtype_codegen() {
        let o = expand(parse_quote! { #[boil(newtype)] struct Name(String); });
        has(
            &o,
            "impl ::core::ops::Deref for Name { type Target = String;",
        );
        has(&o, "impl ::core::ops::DerefMut for Name");
        has(&o, "impl ::core::convert::AsRef<String> for Name");
        has(&o, "pub fn into_inner(self) -> String");
    }

    #[test]
    fn registry_codegen() {
        let o = expand(parse_quote! { #[boil(registry)] enum K { A, B, C } });
        has(
            &o,
            "pub const ALL: &'static [Self] = &[Self::A, Self::B, Self::C]",
        );
        has(&o, "pub const COUNT: usize = 3usize");
        has(&o, "Self::C => 2usize");
        has(&o, "pub const fn variant_name(&self) -> &'static str");
    }

    #[test]
    fn config_codegen() {
        let o = expand(parse_quote! {
            #[boil(config(prefix = "APP_"))] struct Cfg { host: String, #[boil(default = 8080)] port: u16 }
        });
        has(
            &o,
            "pub fn from_env() -> ::core::result::Result<Self, ::std::string::String>",
        );
        has(&o, "pub fn from_lookup");
        has(&o, "\"APP_HOST\"");
        has(&o, "\"APP_PORT\"");
        has(&o, "8080");
    }

    #[test]
    fn nested_patterns_on_one_struct() {
        let o = expand(parse_quote! {
            #[boil(builder, getters, setters, new)] struct U { id: u64, name: String }
        });
        for n in ["UBuilder", "fnid(&self)", "fnset_name", "pubfnnew("] {
            has(&o, n);
        }
    }

    #[test]
    fn generics_flow_through() {
        let o = expand(parse_quote! { #[boil(getters, builder)] struct W<T: Clone> { t: T } });
        has(&o, "impl<T: Clone> W<T>");
        has(&o, "struct WBuilder<T: Clone>");
    }

    #[test]
    fn edge_case_errors() {
        assert!(expand_err(parse_quote! { struct S { a: u8 } }).contains("at least one pattern"));
        assert!(
            expand_err(parse_quote! { #[boil(builder)] struct S(u8); }).contains("named fields")
        );
        assert!(
            expand_err(parse_quote! { #[boil(enum_str)] enum E { A(u8) } })
                .contains("unit variants")
        );
        assert!(expand_err(parse_quote! { #[boil(convert)] struct W<T>(T); }).contains("generic"));
        assert!(
            expand_err(parse_quote! { #[boil(getters)] struct S { #[boil(wat)] a: u8 } })
                .contains("wat")
        );
    }

    #[test]
    fn empty_struct_and_enum() {
        let o = expand(parse_quote! { #[boil(builder, getters, new)] struct E {} });
        has(&o, "pub fn new() -> Self");
        let o = expand(parse_quote! { #[boil(registry, enum_str)] enum Z {} });
        has(&o, "pub const COUNT: usize = 0usize");
    }
}
