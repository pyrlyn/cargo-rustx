//! Declarations of every pattern: what it is called, what it accepts and what it generates.
//!
//! This is the single source of truth used for validation and error messages; the parser
//! maps attribute names onto these rules and the codegen modules implement them.

use syn::{Data, DeriveInput, Fields};

/// The item shape a rule accepts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    /// `struct S { a: A, .. }`
    NamedStruct,
    /// `struct S(Inner);`
    TupleNewtype,
    /// `enum E { A, B }` (unit variants only)
    UnitEnum,
    /// Any `enum`.
    Enum,
    /// An inherent `impl` block (attribute macro).
    ImplBlock,
}

/// Declaration of one pattern. `description`/`generates` are documentation read by tests.
#[derive(Debug)]
#[cfg_attr(not(test), allow(dead_code))]
pub(crate) struct Rule {
    /// Name used inside `#[boil(...)]` (or the attribute name for `bind`).
    pub name: &'static str,
    /// One-line description of the boilerplate it removes.
    pub description: &'static str,
    /// Accepted attribute syntax (container, field and variant level).
    pub syntax: &'static str,
    /// Item shape it applies to.
    pub shape: Shape,
    /// Whether generic parameters on the item are supported.
    pub generics: bool,
    /// What gets generated.
    pub generates: &'static str,
}

/// The registry of all patterns.
pub(crate) const RULES: &[Rule] = &[
    Rule {
        name: "builder",
        description: "Builder type with chained setters and a fallible build()",
        syntax: "#[boil(builder)]; field: #[boil(default = EXPR)] | #[boil(skip)]",
        shape: Shape::NamedStruct,
        generics: true,
        generates: "struct <Name>Builder, <Name>::builder(), one setter per field, build() -> Result<Name, String>",
    },
    Rule {
        name: "getters",
        description: "Borrowing getters for every field",
        syntax: "#[boil(getters)]; field: #[boil(skip)]",
        shape: Shape::NamedStruct,
        generics: true,
        generates: "pub fn <field>(&self) -> &<Ty>",
    },
    Rule {
        name: "setters",
        description: "Chainable &mut setters for every field",
        syntax: "#[boil(setters)]; field: #[boil(skip)]",
        shape: Shape::NamedStruct,
        generics: true,
        generates: "pub fn set_<field>(&mut self, v: impl Into<Ty>) -> &mut Self",
    },
    Rule {
        name: "new",
        description: "Positional constructor",
        syntax: "#[boil(new)]; field: #[boil(default = EXPR)] | #[boil(skip)] (excluded, Default::default())",
        shape: Shape::NamedStruct,
        generics: true,
        generates: "pub fn new(<field>: impl Into<Ty>, ..) -> Self",
    },
    Rule {
        name: "error",
        description: "Error enum with Display messages and From conversions",
        syntax: "#[boil(error)]; variant: #[boil(msg = \"...{field}...\")], #[boil(from)] on single-field tuple variants (bound as _0)",
        shape: Shape::Enum,
        generics: false,
        generates: "impl Display, impl std::error::Error, impl From<Inner> for #[boil(from)] variants",
    },
    Rule {
        name: "enum_str",
        description: "Enum <-> string conversion",
        syntax: "#[boil(enum_str)]; variant: #[boil(rename = \"...\")]",
        shape: Shape::UnitEnum,
        generics: false,
        generates: "as_str(), impl Display, impl FromStr<Err = String>",
    },
    Rule {
        name: "convert",
        description: "From conversions between a newtype and its inner value",
        syntax: "#[boil(convert)]",
        shape: Shape::TupleNewtype,
        generics: false,
        generates: "impl From<Inner> for Name, impl From<Name> for Inner",
    },
    Rule {
        name: "newtype",
        description: "Newtype delegation to the inner value",
        syntax: "#[boil(newtype)]",
        shape: Shape::TupleNewtype,
        generics: false,
        generates: "impl Deref/DerefMut<Target = Inner>, impl AsRef<Inner>, into_inner()",
    },
    Rule {
        name: "registry",
        description: "Static registry of all enum variants",
        syntax: "#[boil(registry)]",
        shape: Shape::UnitEnum,
        generics: false,
        generates: "const ALL: &[Self], const COUNT: usize, index(), variant_name()",
    },
    Rule {
        name: "config",
        description: "Config struct loaded from environment-like key/value sources",
        syntax: "#[boil(config)] | #[boil(config(prefix = \"APP_\"))]; field: #[boil(default = EXPR)]",
        shape: Shape::NamedStruct,
        generics: false,
        generates: "from_env() and from_lookup(fn) -> Result<Self, String>; key = PREFIX + FIELD_UPPERCASE, values parsed via FromStr",
    },
    Rule {
        name: "bind",
        description: "Bind inherent methods to a trait",
        syntax: "#[bind(path::to::Trait)] impl Type { fn ..(&self, ..) {..} }",
        shape: Shape::ImplBlock,
        generics: true,
        generates: "the original impl block plus impl Trait for Type delegating every method",
    },
];

/// Find a rule by name.
pub(crate) fn lookup(name: &str) -> Option<&'static Rule> {
    RULES.iter().find(|r| r.name == name)
}

/// Comma-separated names of the derive patterns (for error messages).
pub(crate) fn names() -> String {
    RULES
        .iter()
        .filter(|r| r.shape != Shape::ImplBlock)
        .map(|r| r.name)
        .collect::<Vec<_>>()
        .join(", ")
}

/// Validate that `input` has the shape `rule` requires.
pub(crate) fn check(rule: &Rule, input: &DeriveInput) -> syn::Result<()> {
    let fail = |what: &str| {
        Err(syn::Error::new_spanned(
            &input.ident,
            format!("`{}` pattern expects {what} ({})", rule.name, rule.syntax),
        ))
    };
    if !rule.generics && !input.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input.generics,
            format!(
                "`{}` pattern does not support generic parameters",
                rule.name
            ),
        ));
    }
    let ok = match (&input.data, rule.shape) {
        (Data::Struct(s), Shape::NamedStruct) => matches!(s.fields, Fields::Named(_)),
        (Data::Struct(s), Shape::TupleNewtype) => {
            matches!(&s.fields, Fields::Unnamed(f) if f.unnamed.len() == 1)
        }
        (Data::Enum(e), Shape::UnitEnum) => {
            e.variants.iter().all(|v| matches!(v.fields, Fields::Unit))
        }
        (Data::Enum(_), Shape::Enum) => true,
        _ => false,
    };
    if ok {
        return Ok(());
    }
    match rule.shape {
        Shape::NamedStruct => fail("a struct with named fields"),
        Shape::TupleNewtype => fail("a tuple struct with exactly one field"),
        Shape::UnitEnum => fail("an enum with only unit variants"),
        Shape::Enum => fail("an enum"),
        Shape::ImplBlock => fail("an impl block"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::parse_quote;

    #[test]
    fn registry_has_all_patterns_unique() {
        assert_eq!(RULES.len(), 11);
        let mut names: Vec<_> = RULES.iter().map(|r| r.name).collect();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), RULES.len());
        for r in RULES {
            assert!(!r.description.is_empty() && !r.syntax.is_empty() && !r.generates.is_empty());
        }
    }

    #[test]
    fn names_excludes_attribute_macros() {
        assert!(names().contains("builder"));
        assert!(!names().contains("bind"));
    }

    #[test]
    fn shape_checks() {
        let named: DeriveInput = parse_quote!(
            struct S {
                a: u8,
            }
        );
        let tuple: DeriveInput = parse_quote!(
            struct T(u8);
        );
        let unit: DeriveInput = parse_quote!(
            enum E {
                A,
                B,
            }
        );
        let data: DeriveInput = parse_quote!(
            enum D {
                A(u8),
            }
        );
        let builder = lookup("builder").unwrap();
        let newtype = lookup("newtype").unwrap();
        let es = lookup("enum_str").unwrap();
        let err = lookup("error").unwrap();
        assert!(check(builder, &named).is_ok());
        assert!(check(builder, &tuple).is_err());
        assert!(check(newtype, &tuple).is_ok());
        assert!(check(newtype, &named).is_err());
        assert!(check(es, &unit).is_ok());
        assert!(check(es, &data).is_err());
        assert!(check(err, &data).is_ok());
        assert!(check(err, &named).is_err());
    }

    #[test]
    fn generics_rejected_where_declared() {
        let g: DeriveInput = parse_quote!(
            struct W<T>(T);
        );
        let e = check(lookup("convert").unwrap(), &g).unwrap_err();
        assert!(e.to_string().contains("generic"));
        let b: DeriveInput = parse_quote!(
            struct B<T> {
                t: T,
            }
        );
        assert!(check(lookup("builder").unwrap(), &b).is_ok());
    }
}
