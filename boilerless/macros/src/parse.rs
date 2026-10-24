//! Parsing of `#[boil(...)]` attributes into typed configs.

use syn::{Attribute, DeriveInput, Expr, Field, LitStr, Variant};

use crate::rules;

/// A pattern requested on the container.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum Pattern {
    Builder,
    Getters,
    Setters,
    New,
    Error,
    EnumStr,
    Convert,
    Newtype,
    Registry,
    Config { prefix: String },
}

impl Pattern {
    /// The rule name of this pattern (see [`rules::RULES`]).
    pub(crate) fn name(&self) -> &'static str {
        match self {
            Self::Builder => "builder",
            Self::Getters => "getters",
            Self::Setters => "setters",
            Self::New => "new",
            Self::Error => "error",
            Self::EnumStr => "enum_str",
            Self::Convert => "convert",
            Self::Newtype => "newtype",
            Self::Registry => "registry",
            Self::Config { .. } => "config",
        }
    }
}

/// Container-level configuration.
#[derive(Debug, Default)]
pub(crate) struct ContainerConfig {
    pub patterns: Vec<Pattern>,
}

/// Field-level configuration.
#[derive(Debug, Default)]
pub(crate) struct FieldConfig {
    pub skip: bool,
    pub default: Option<Expr>,
}

/// Variant-level configuration.
#[derive(Debug, Default)]
pub(crate) struct VariantConfig {
    pub msg: Option<LitStr>,
    pub rename: Option<LitStr>,
    pub from: bool,
}

fn boil_attrs(attrs: &[Attribute]) -> impl Iterator<Item = &Attribute> {
    attrs.iter().filter(|a| a.path().is_ident("boil"))
}

fn ident_of(path: &syn::Path) -> String {
    path.get_ident()
        .map(ToString::to_string)
        .unwrap_or_default()
}

/// Parse all `#[boil(...)]` attributes on the item itself.
pub(crate) fn parse_container(input: &DeriveInput) -> syn::Result<ContainerConfig> {
    let mut patterns: Vec<Pattern> = Vec::new();
    for attr in boil_attrs(&input.attrs) {
        attr.parse_nested_meta(|meta| {
            let name = ident_of(&meta.path);
            let pattern = match name.as_str() {
                "builder" => Pattern::Builder,
                "getters" => Pattern::Getters,
                "setters" => Pattern::Setters,
                "new" => Pattern::New,
                "error" => Pattern::Error,
                "enum_str" => Pattern::EnumStr,
                "convert" => Pattern::Convert,
                "newtype" => Pattern::Newtype,
                "registry" => Pattern::Registry,
                "config" => {
                    let mut prefix = String::new();
                    if meta.input.peek(syn::token::Paren) {
                        meta.parse_nested_meta(|m| {
                            if m.path.is_ident("prefix") {
                                prefix = m.value()?.parse::<LitStr>()?.value();
                                Ok(())
                            } else {
                                Err(m.error("unknown `config` option; expected `prefix = \"...\"`"))
                            }
                        })?;
                    }
                    Pattern::Config { prefix }
                }
                _ => {
                    return Err(meta.error(format!(
                        "unknown boil pattern `{name}`; expected one of: {}",
                        rules::names()
                    )));
                }
            };
            if patterns.iter().any(|p| p.name() == pattern.name()) {
                return Err(meta.error(format!("duplicate boil pattern `{name}`")));
            }
            patterns.push(pattern);
            Ok(())
        })?;
    }
    Ok(ContainerConfig { patterns })
}

/// Parse `#[boil(...)]` on a struct field.
pub(crate) fn parse_field(field: &Field) -> syn::Result<FieldConfig> {
    let mut cfg = FieldConfig::default();
    for attr in boil_attrs(&field.attrs) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("skip") {
                cfg.skip = true;
            } else if meta.path.is_ident("default") {
                cfg.default = Some(meta.value()?.parse::<Expr>()?);
            } else {
                return Err(meta.error(format!(
                    "unknown field option `{}`; expected `skip` or `default = EXPR`",
                    ident_of(&meta.path)
                )));
            }
            Ok(())
        })?;
    }
    Ok(cfg)
}

/// Parse `#[boil(...)]` on an enum variant.
pub(crate) fn parse_variant(variant: &Variant) -> syn::Result<VariantConfig> {
    let mut cfg = VariantConfig::default();
    for attr in boil_attrs(&variant.attrs) {
        attr.parse_nested_meta(|meta| {
            if meta.path.is_ident("msg") {
                cfg.msg = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("rename") {
                cfg.rename = Some(meta.value()?.parse()?);
            } else if meta.path.is_ident("from") {
                cfg.from = true;
            } else {
                return Err(meta.error(format!(
                    "unknown variant option `{}`; expected `msg = \"..\"`, `rename = \"..\"` or `from`",
                    ident_of(&meta.path)
                )));
            }
            Ok(())
        })?;
    }
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    fn patterns(input: DeriveInput) -> syn::Result<Vec<Pattern>> {
        parse_container(&input).map(|c| c.patterns)
    }

    #[test]
    fn parses_every_pattern() {
        let p = patterns(parse_quote! {
            #[boil(builder, getters, setters, new, error)]
            #[boil(enum_str, convert, newtype, registry, config)]
            struct S;
        })
        .unwrap();
        let names: Vec<_> = p.iter().map(Pattern::name).collect();
        assert_eq!(
            names,
            [
                "builder", "getters", "setters", "new", "error", "enum_str", "convert", "newtype",
                "registry", "config"
            ]
        );
        for n in names {
            assert!(rules::lookup(n).is_some(), "{n} has a rule");
        }
    }

    #[test]
    fn parses_config_prefix() {
        let p = patterns(parse_quote! { #[boil(config(prefix = "APP_"))] struct S; }).unwrap();
        assert_eq!(
            p,
            [Pattern::Config {
                prefix: "APP_".into()
            }]
        );
        let p = patterns(parse_quote! { #[boil(config)] struct S; }).unwrap();
        assert_eq!(
            p,
            [Pattern::Config {
                prefix: String::new()
            }]
        );
    }

    #[test]
    fn no_attributes_is_empty() {
        assert!(
            patterns(parse_quote! { struct S { a: u8 } })
                .unwrap()
                .is_empty()
        );
        assert!(
            patterns(parse_quote! { #[derive(Debug)] #[boil()] struct S; })
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn rejects_unknown_pattern() {
        let e = patterns(parse_quote! { #[boil(bulder)] struct S; })
            .unwrap_err()
            .to_string();
        assert!(e.contains("unknown boil pattern `bulder`"), "{e}");
        assert!(e.contains("builder"), "{e}");
    }

    #[test]
    fn rejects_duplicate_and_bad_config_option() {
        let e = patterns(parse_quote! { #[boil(builder, builder)] struct S; }).unwrap_err();
        assert!(e.to_string().contains("duplicate"));
        let e = patterns(parse_quote! { #[boil(config(prefx = "A"))] struct S; }).unwrap_err();
        assert!(e.to_string().contains("unknown `config` option"));
        let e = patterns(parse_quote! { #[boil(config(prefix = 1))] struct S; }).unwrap_err();
        assert!(e.to_string().contains("expected string literal"), "{e}");
    }

    #[test]
    fn rejects_invalid_syntax() {
        assert!(patterns(parse_quote! { #[boil(builder = )] struct S; }).is_err());
        assert!(patterns(parse_quote! { #[boil = "builder"] struct S; }).is_err());
    }

    #[test]
    fn parses_field_options() {
        let f: Field = parse_quote! { #[boil(default = 8080)] port: u16 };
        let c = parse_field(&f).unwrap();
        assert!(!c.skip);
        assert!(c.default.is_some());
        let f: Field = parse_quote! { #[boil(skip)] cache: u8 };
        assert!(parse_field(&f).unwrap().skip);
        let f: Field = parse_quote! { plain: u8 };
        let c = parse_field(&f).unwrap();
        assert!(!c.skip && c.default.is_none());
        let f: Field = parse_quote! { #[boil(nope)] x: u8 };
        assert!(
            parse_field(&f)
                .unwrap_err()
                .to_string()
                .contains("unknown field option `nope`")
        );
    }

    #[test]
    fn parses_variant_options() {
        let v: Variant = parse_quote! { #[boil(msg = "io: {_0}", from)] Io(std::io::Error) };
        let c = parse_variant(&v).unwrap();
        assert_eq!(c.msg.unwrap().value(), "io: {_0}");
        assert!(c.from);
        let v: Variant = parse_quote! { #[boil(rename = "red")] Red };
        assert_eq!(parse_variant(&v).unwrap().rename.unwrap().value(), "red");
        let v: Variant = parse_quote! { #[boil(mesage = "x")] A };
        assert!(parse_variant(&v).is_err());
    }
}
