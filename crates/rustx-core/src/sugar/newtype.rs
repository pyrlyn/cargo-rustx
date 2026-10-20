//! `new` and the flag impls back to `#[newtype(..)]`: the reverse of `passes/newtype.rs`
//! (problem 20; T19.18).
//!
//! A one-field tuple struct followed directly by the impls the pass generates for it,
//!
//! ```text
//! pub struct Meters(f64);
//! #[allow(dead_code)]
//! impl Meters { pub fn new(__rustx_inner: f64) -> Self { Self(__rustx_inner) } }
//! impl ::core::ops::Add for Meters { .. }
//! ```
//!
//! becomes the struct with `#[newtype(add)]` and no impls. The impls after `new` name their flags
//! by their traits (`Deref`, `AsRef`, `Display`, `FromStr`, `From` for `into`, `Add` to `Rem`), in
//! the order written, which is the order the pass generates them in; a `Display` that writes a
//! string gives `display = "lit"`, and a `new` that first calls `f(&inner)?` gives `validate = f`.
//! Nothing else of the impls is matched here: the rule expands the struct it would write, next to
//! the validator, and keeps it only when that gives back the struct and exactly those impls. When
//! it does not, it tries again without the last impl, so a hand-written impl after the generated
//! ones stays as written. When not even `new` alone reads back (another method, a doc or an
//! attribute, other bounds, a validator whose signature the pass reads differently), the struct
//! and its impls stay.
//!
//! The attribute goes after the struct's doc comments, before its other attributes. Refused as
//! well: an impl that is not right after its struct or the impl before it, and anything that would
//! drop a comment between the struct and its last impl removed.

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::punctuated::Punctuated;
use syn::{
    Expr, ExprLit, Fields, Ident, ImplItem, Item, ItemFn, ItemImpl, ItemStruct, Lit, Stmt, Token,
    Visibility,
};

use super::std_paths::StdPaths;
use super::{Edit, ReverseRule, has_comment, item_lists, span_range};

pub(crate) struct Newtypes;

impl ReverseRule for Newtypes {
    fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit> {
        self.edit_groups(file, source).concat()
    }

    /// The attribute and the removal of the impls, one set per struct: either alone changes the
    /// tokens.
    fn edit_groups(&self, file: &syn::File, source: &str) -> Vec<Vec<Edit>> {
        let paths = StdPaths::of(file);
        let units = Units {
            source,
            paths: &paths,
        };
        item_lists(file)
            .iter()
            .flat_map(|list| {
                list.iter()
                    .enumerate()
                    .filter_map(|(at, item)| match item {
                        Some(Item::Struct(item)) => units.unit(item, &list[at + 1..], list),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
            })
            .collect()
    }
}

struct Units<'a> {
    source: &'a str,
    paths: &'a StdPaths,
}

impl Units<'_> {
    /// The edits for `item` and the impls among the items `after` it, in the item list `list`.
    fn unit(
        &self,
        item: &ItemStruct,
        after: &[Option<&Item>],
        list: &[Option<&Item>],
    ) -> Option<Vec<Edit>> {
        if !matches!(&item.fields, Fields::Unnamed(fields) if fields.unnamed.len() == 1) {
            return None;
        }
        let impls: Vec<&ItemImpl> = after
            .iter()
            .map_while(|item| match item {
                Some(Item::Impl(imp)) => Some(imp),
                _ => None,
            })
            .collect();
        let (constructor, rest) = impls.split_first()?;
        let validator = match validator(constructor) {
            Some(name) => Some(sibling_fn(list, name)?),
            None => None,
        };
        let mut flags: Vec<(&str, String)> = Vec::new();
        for imp in rest {
            match flag(imp) {
                Some(flag) if flags.iter().all(|(name, _)| *name != flag.0) => flags.push(flag),
                _ => break,
            }
        }
        // The most impls the attribute gives back, `new` always among them.
        let (attr, last_impl) = (0..=flags.len()).rev().find_map(|count| {
            let attr = attribute(&flags[..count], validator);
            self.reads_back(&attr, item, &impls[..=count], validator)
                .then_some((attr, impls[count]))
        })?;

        let struct_end = span_range(item.to_token_stream())?.end;
        let impls_end = span_range(last_impl.to_token_stream())?.end;
        if has_comment(self.source.get(struct_end..impls_end)?) {
            return None;
        }
        let at = span_range(anchor(item))?.start;
        let line = self.source.get(..at)?.rfind('\n').map_or(0, |end| end + 1);
        let indent = self.source.get(line..at)?;
        let text = if indent.trim().is_empty() {
            format!("{attr}\n{indent}")
        } else {
            format!("{attr} ")
        };
        Some(vec![
            Edit {
                range: at..at,
                text,
            },
            Edit {
                range: struct_end..impls_end,
                text: String::new(),
            },
        ])
    }

    /// Whether `attr` on `item`, next to `validator`, expands to exactly `item`, `impls` and the
    /// validator.
    fn reads_back(
        &self,
        attr: &str,
        item: &ItemStruct,
        impls: &[&ItemImpl],
        validator: Option<&ItemFn>,
    ) -> bool {
        let Ok(attr) = attr.parse::<TokenStream>() else {
            return false;
        };
        crate::try_expand(quote!(#attr #item #validator)).is_ok_and(|file| {
            self.paths.normalise(file.into_token_stream())
                == self.paths.normalise(quote!(#item #(#impls)* #validator))
        })
    }
}

/// Where the attribute goes: before the struct's first attribute that is not its doc comment.
fn anchor(item: &ItemStruct) -> TokenStream {
    let attr = item.attrs.iter().find(|attr| !attr.path().is_ident("doc"));
    match (attr, &item.vis) {
        (Some(attr), _) => attr.to_token_stream(),
        (None, Visibility::Inherited) => item.struct_token.to_token_stream(),
        (None, vis) => vis.to_token_stream(),
    }
}

/// `f` of a `new` whose first statement is `f(&inner)?;`, as with `validate = f`.
fn validator(constructor: &ItemImpl) -> Option<&Ident> {
    let [ImplItem::Fn(new)] = constructor.items.as_slice() else {
        return None;
    };
    let [Stmt::Expr(Expr::Try(check), Some(_)), _] = new.block.stmts.as_slice() else {
        return None;
    };
    let Expr::Call(call) = &*check.expr else {
        return None;
    };
    let Expr::Path(path) = &*call.func else {
        return None;
    };
    path.path.get_ident()
}

/// The fn `name` of the item list, where the pass reads a validator's error type from.
fn sibling_fn<'i>(list: &[Option<&'i Item>], name: &Ident) -> Option<&'i ItemFn> {
    list.iter().find_map(|item| match item {
        Some(Item::Fn(f)) if f.sig.ident == *name => Some(f),
        _ => None,
    })
}

/// The flag an impl's trait stands for, and how the attribute writes it.
fn flag(imp: &ItemImpl) -> Option<(&'static str, String)> {
    let (path, _) = imp.trait_.as_ref()?;
    let name = match path.segments.last()?.ident.to_string().as_str() {
        "Deref" => "deref",
        "AsRef" => "as_ref",
        "Display" => "display",
        "FromStr" => "from_str",
        "From" => "into",
        "Add" => "add",
        "Sub" => "sub",
        "Mul" => "mul",
        "Div" => "div",
        "Rem" => "rem",
        _ => return None,
    };
    let text = match display_format(imp) {
        Some(format) if name == "display" => format!("display = {format}"),
        _ => name.to_owned(),
    };
    Some((name, text))
}

/// The string literal of a `fmt` that is one `write!(f, "..", ..)`, as written.
fn display_format(imp: &ItemImpl) -> Option<String> {
    let [ImplItem::Fn(fmt)] = imp.items.as_slice() else {
        return None;
    };
    let mac = match fmt.block.stmts.as_slice() {
        [Stmt::Macro(stmt)] => &stmt.mac,
        [Stmt::Expr(Expr::Macro(expr), None)] => &expr.mac,
        _ => return None,
    };
    let args = mac
        .parse_body_with(Punctuated::<Expr, Token![,]>::parse_terminated)
        .ok()?;
    match args.iter().nth(1)? {
        Expr::Lit(ExprLit {
            lit: Lit::Str(lit), ..
        }) => Some(lit.token().to_string()),
        _ => None,
    }
}

/// `#[newtype(flags, validate = f)]`, or `#[newtype]` with neither.
fn attribute(flags: &[(&str, String)], validator: Option<&ItemFn>) -> String {
    let mut written: Vec<String> = flags.iter().map(|(_, text)| text.clone()).collect();
    if let Some(validator) = validator {
        written.push(format!("validate = {}", validator.sig.ident));
    }
    if written.is_empty() {
        "#[newtype]".to_owned()
    } else {
        format!("#[newtype({})]", written.join(", "))
    }
}

#[cfg(test)]
mod tests {
    use super::Newtypes;
    use crate::sugar::tests::assert_sugar;
    use crate::sugar::{Edit, ReverseRule, convert};

    /// The inherent impl the pass generates for `Id(u64)` without a validator.
    const NEW: &str = "#[allow(dead_code)]\nimpl Id {\n    pub fn new(__rustx_inner: u64) -> Self {\n        \
                       Self(__rustx_inner)\n    }\n}\n";

    /// This rule proposes no edit for `source`, and the driver with it alone changes nothing;
    /// other rules may still edit it.
    fn unchanged(source: &str) {
        let file = syn::parse_file(source).unwrap();
        assert_eq!(Newtypes.edits(&file, source), Vec::<Edit>::new());
        assert_eq!(convert(source, &[&Newtypes]).unwrap(), source);
    }

    /// The impl the pass generates for `flag` on `Id(u64)`.
    fn generated(flag: &str) -> &'static str {
        match flag {
            "deref" => {
                "impl ::core::ops::Deref for Id {\n    type Target = u64;\n    \
                 fn deref(&self) -> &u64 {\n        &self.0\n    }\n}\n"
            }
            "as_ref" => {
                "impl ::core::convert::AsRef<u64> for Id {\n    \
                 fn as_ref(&self) -> &u64 {\n        &self.0\n    }\n}\n"
            }
            "display" => {
                "impl ::core::fmt::Display for Id {\n    \
                 fn fmt(&self, __rustx_f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {\n        \
                 ::core::fmt::Display::fmt(&self.0, __rustx_f)\n    }\n}\n"
            }
            "from_str" => {
                "impl ::core::str::FromStr for Id {\n    \
                 type Err = <u64 as ::core::str::FromStr>::Err;\n    \
                 fn from_str(__rustx_s: &str) -> ::core::result::Result<Self, Self::Err> {\n        \
                 <u64 as ::core::str::FromStr>::from_str(__rustx_s).map(Self)\n    }\n}\n"
            }
            "into" => {
                "impl ::core::convert::From<Id> for u64 {\n    \
                 fn from(__rustx_value: Id) -> Self {\n        __rustx_value.0\n    }\n}\n"
            }
            _ => unreachable!("{flag}"),
        }
    }

    fn operator(trait_name: &str, method: &str) -> String {
        format!(
            "impl ::core::ops::{trait_name} for Id {{\n    type Output = Self;\n    \
             fn {method}(self, __rustx_other: Self) -> Self {{\n        \
             Self(::core::ops::{trait_name}::{method}(self.0, __rustx_other.0))\n    }}\n}}\n"
        )
    }

    #[test]
    fn a_struct_with_only_the_generated_new_becomes_a_bare_newtype() {
        assert_sugar(
            &format!("#[derive(Debug)]\nstruct Id(u64);\n{NEW}fn after() {{}}\n"),
            "#[newtype]\n#[derive(Debug)]\nstruct Id(u64);\nfn after() {}\n",
        );
    }

    #[test]
    fn every_flag_alone_converts() {
        for flag in ["deref", "as_ref", "display", "from_str", "into"] {
            assert_sugar(
                &format!("struct Id(u64);\n{NEW}{}", generated(flag)),
                &format!("#[newtype({flag})]\nstruct Id(u64);\n"),
            );
        }
        for (trait_name, method) in [
            ("Add", "add"),
            ("Sub", "sub"),
            ("Mul", "mul"),
            ("Div", "div"),
            ("Rem", "rem"),
        ] {
            assert_sugar(
                &format!("struct Id(u64);\n{NEW}{}", operator(trait_name, method)),
                &format!("#[newtype({method})]\nstruct Id(u64);\n"),
            );
        }
    }

    #[test]
    fn all_flags_convert_in_the_order_their_impls_are_written() {
        let flags = ["into", "display", "from_str", "deref", "as_ref"];
        let impls: String = flags.iter().map(|flag| generated(flag)).collect();
        let ops = operator("Mul", "mul") + &operator("Add", "add");
        assert_sugar(
            &format!("pub struct Id(u64);\n{NEW}{impls}{ops}"),
            "#[newtype(into, display, from_str, deref, as_ref, mul, add)]\npub struct Id(u64);\n",
        );
    }

    #[test]
    fn the_problem_example_with_a_validator_converts() {
        // The `throws` rule converts the validator too; it expands back before the pass reads it.
        assert_sugar(
            "/// An address.\n#[derive(Debug)]\npub struct Email(String);\n\
             #[allow(dead_code)]\nimpl Email {\n    \
             pub fn new(__rustx_inner: String) -> ::core::result::Result<Self, &'static str> {\n        \
             check(&__rustx_inner)?;\n        ::core::result::Result::Ok(Self(__rustx_inner))\n    }\n}\n\
             impl ::core::ops::Deref for Email {\n    type Target = str;\n    \
             fn deref(&self) -> &str {\n        &self.0\n    }\n}\n\
             impl ::core::str::FromStr for Email {\n    type Err = &'static str;\n    \
             fn from_str(__rustx_s: &str) -> ::core::result::Result<Self, Self::Err> {\n        \
             Self::new(::core::convert::From::from(__rustx_s))\n    }\n}\n\
             fn check(s: &String) -> Result<(), &'static str> {\n    todo!()\n}\n",
            "/// An address.\n#[newtype(deref, from_str, validate = check)]\n#[derive(Debug)]\n\
             pub struct Email(String);\n\
             fn check(s: &String) -> () throws &'static str {\n    todo!()\n}\n",
        );
    }

    #[test]
    fn a_validated_number_parses_before_its_check() {
        assert_sugar(
            "fn nonzero(p: &u16) -> Result<(), Bad> { todo!() }\nstruct Port(u16);\n\
             #[allow(dead_code)]\nimpl Port { pub fn new(__rustx_inner: u16) -> ::core::result::Result<Self, Bad> \
             { nonzero(&__rustx_inner)?; ::core::result::Result::Ok(Self(__rustx_inner)) } }\n\
             impl ::core::str::FromStr for Port { type Err = Bad; \
             fn from_str(__rustx_s: &str) -> ::core::result::Result<Self, Self::Err> \
             { Self::new(<u16 as ::core::str::FromStr>::from_str(__rustx_s)?) } }\n",
            "fn nonzero(p: &u16) -> () throws Bad { todo!() }\n\
             #[newtype(from_str, validate = nonzero)]\nstruct Port(u16);\n",
        );
    }

    #[test]
    fn a_display_writing_a_string_becomes_display_with_that_string() {
        let display = |write: &str| {
            format!(
                "impl ::core::fmt::Display for Id {{ fn fmt(&self, __rustx_f: &mut \
                 ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {{ {write} }} }}\n"
            )
        };
        assert_sugar(
            &format!(
                "struct Id(u64);\n{NEW}{}",
                display("::core::write!(__rustx_f, \"{0} m\", &self.0)")
            ),
            "#[newtype(display = \"{0} m\")]\nstruct Id(u64);\n",
        );
        assert_sugar(
            &format!(
                "struct Id(u64);\n{NEW}{}",
                display("::core::write!(__rustx_f, \"an id\")")
            ),
            "#[newtype(display = \"an id\")]\nstruct Id(u64);\n",
        );
    }

    #[test]
    fn short_std_names_convert() {
        assert_sugar(
            &format!(
                "use std::ops::Deref;\nstruct Id(u64);\n{NEW}impl Deref for Id {{\n    \
                 type Target = u64;\n    fn deref(&self) -> &u64 {{ &self.0 }}\n}}\n"
            ),
            "use std::ops::Deref;\n#[newtype(deref)]\nstruct Id(u64);\n",
        );
    }

    #[test]
    fn generics_in_a_module_and_a_vec_target_convert() {
        assert_sugar(
            "mod m {\n    pub struct List<T: Clone>(Vec<T>) where T: Default;\n    \
             #[allow(dead_code)]\n    impl<T: Clone> List<T> where T: Default {\n        \
             pub fn new(__rustx_inner: Vec<T>) -> Self { Self(__rustx_inner) }\n    }\n    \
             impl<T: Clone> ::core::ops::Deref for List<T> where T: Default {\n        \
             type Target = [T];\n        fn deref(&self) -> &[T] { &self.0 }\n    }\n}\n",
            "mod m {\n    #[newtype(deref)]\n    \
             pub struct List<T: Clone>(Vec<T>) where T: Default;\n}\n",
        );
    }

    #[test]
    fn a_struct_after_other_code_on_its_line_gets_the_attribute_inline() {
        assert_sugar(
            &format!("fn f() {{}} pub struct Id(u64);\n{NEW}"),
            "fn f() {} #[newtype] pub struct Id(u64);\n",
        );
    }

    #[test]
    fn comment_markers_inside_a_display_string_do_not_block_it() {
        assert_sugar(
            "pub struct Url(String);\n#[allow(dead_code)]\nimpl Url { pub fn new(__rustx_inner: String) -> \
             Self { Self(__rustx_inner) } }\nimpl ::core::fmt::Display for Url { fn fmt(&self, \
             __rustx_f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { \
             ::core::write!(__rustx_f, \"see http://x/{0} /*\", &self.0) } }\n",
            "#[newtype(display = \"see http://x/{0} /*\")]\npub struct Url(String);\n",
        );
    }

    #[test]
    fn a_struct_in_a_fn_body_converts() {
        assert_sugar(
            "fn f() {\n    struct Id(u64);\n    #[allow(dead_code)]\n    \
             impl Id { pub fn new(__rustx_inner: u64) -> Self { Self(__rustx_inner) } }\n    \
             let x = 1;\n}\n",
            "fn f() {\n    #[newtype]\n    struct Id(u64);\n    let x = 1;\n}\n",
        );
    }

    #[test]
    fn a_hand_written_impl_after_the_generated_ones_stays() {
        let display = "impl ::core::fmt::Display for Id {\n    \
                       fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {\n        \
                       write!(f, \"#{}\", self.0)\n    }\n}\n";
        assert_sugar(
            &format!("struct Id(u64);\n{NEW}{}{display}", generated("deref")),
            &format!("#[newtype(deref)]\nstruct Id(u64);\n{display}"),
        );
        // A second impl of the same trait is never a second flag.
        assert_sugar(
            &format!(
                "struct Id(u64);\n{NEW}{}{}",
                generated("into"),
                generated("into")
            ),
            &format!("#[newtype(into)]\nstruct Id(u64);\n{}", generated("into")),
        );
    }

    #[test]
    fn an_impl_the_pass_would_not_generate_is_left_alone() {
        // Another method, or a parameter named otherwise.
        unchanged(&format!(
            "struct Id(u64);\n{}",
            NEW.replace(
                "    }\n}",
                "    }\n    pub fn get(&self) -> u64 { self.0 }\n}"
            )
        ));
        unchanged(&format!(
            "struct Id(u64);\n{}",
            NEW.replace("__rustx_inner", "inner")
        ));
        // No `#[allow(dead_code)]`, or a doc on the impl.
        unchanged(&format!(
            "struct Id(u64);\n{}",
            NEW.replace("#[allow(dead_code)]\n", "")
        ));
        unchanged(&format!(
            "struct Id(u64);\n/// Builds one.\n{NEW}{}",
            generated("deref")
        ));
        // A bound the pass would not add, and an impl for another type.
        unchanged(
            "struct W<T>(T);\n#[allow(dead_code)]\nimpl<T: Clone> W<T> { pub fn new(__rustx_inner: T) -> Self \
             { Self(__rustx_inner) } }\n",
        );
        unchanged(&format!(
            "struct Id(u64);\n{}",
            NEW.replace("impl Id", "impl Other")
        ));
        // Not a one-field tuple struct.
        unchanged(&format!(
            "struct Id(u64, u8);\n{}",
            NEW.replace("Self(__rustx_inner)", "Self(__rustx_inner, 0)")
        ));
    }

    #[test]
    fn a_validator_the_pass_reads_differently_is_left_alone() {
        let source = |check: &str| {
            format!(
                "{check}\nstruct Port(u16);\n#[allow(dead_code)]\nimpl Port {{ pub fn new(__rustx_inner: u16) -> \
                 ::core::result::Result<Self, Bad> {{ nonzero(&__rustx_inner)?; \
                 ::core::result::Result::Ok(Self(__rustx_inner)) }} }}\n"
            )
        };
        // Another error type, an alias the pass cannot read, no validator in the list, and one
        // in another module.
        unchanged(&source(
            "fn nonzero(p: &u16) -> Result<(), Other> { Ok(()) }",
        ));
        unchanged(&source("fn nonzero(p: &u16) -> io::Result<()> { Ok(()) }"));
        unchanged(&source(""));
        unchanged(&source(
            "mod m { pub fn nonzero(p: &u16) -> Result<(), Bad> { Ok(()) } }\nuse m::nonzero;",
        ));
    }

    #[test]
    fn an_operator_with_a_validator_stays_as_written() {
        // The pass refuses `add` with `validate`, so only `new` converts.
        let add = operator("Add", "add").replace("Id", "Port");
        assert_sugar(
            &format!(
                "fn nonzero(p: &u16) -> Result<(), Bad> {{ todo!() }}\nstruct Port(u16);\n\
                 #[allow(dead_code)]\nimpl Port {{ pub fn new(__rustx_inner: u16) -> \
                 ::core::result::Result<Self, Bad> {{ nonzero(&__rustx_inner)?; \
                 ::core::result::Result::Ok(Self(__rustx_inner)) }} }}\n{add}"
            ),
            &format!(
                "fn nonzero(p: &u16) -> () throws Bad {{ todo!() }}\n\
                 #[newtype(validate = nonzero)]\nstruct Port(u16);\n{add}"
            ),
        );
    }

    #[test]
    fn an_impl_apart_from_its_struct_or_a_comment_it_would_drop_is_left_alone() {
        unchanged(&format!("struct Id(u64);\nfn f() {{}}\n{NEW}"));
        unchanged(&format!("struct Id(u64);\n// why\n{NEW}"));
        unchanged(&format!(
            "struct Id(u64);\n{}",
            NEW.replace("Self(__rustx_inner)", "Self(__rustx_inner) // why")
        ));
        unchanged(&format!(
            "fn f() {{\n    struct Id(u64);\n    let x = 1;\n    {}}}\n",
            NEW.replace('\n', " ")
        ));
        // A comment after the last impl removed survives with the impl that stays.
        let display = "impl ::core::fmt::Display for Id { fn fmt(&self, f: &mut \
                       ::core::fmt::Formatter<'_>) -> ::core::fmt::Result { write!(f, \"x\") } }\n";
        assert_sugar(
            &format!("struct Id(u64);\n{NEW}// hand-written\n{display}"),
            &format!("#[newtype]\nstruct Id(u64);\n// hand-written\n{display}"),
        );
    }

    /// Replaces the first occurrence of a text, whether or not that keeps the meaning.
    struct Replace(&'static str, &'static str);

    impl ReverseRule for Replace {
        fn edits(&self, _: &syn::File, source: &str) -> Vec<Edit> {
            source
                .find(self.0)
                .map(|at| Edit {
                    range: at..at + self.0.len(),
                    text: self.1.to_owned(),
                })
                .into_iter()
                .collect()
        }
    }

    #[test]
    fn the_attribute_and_the_removal_are_one_set_kept_or_dropped_whole() {
        let source = format!("struct Id(u64);\n{NEW}");
        let file = syn::parse_file(&source).unwrap();
        assert_eq!(Newtypes.edit_groups(&file, &source).len(), 1);
        assert_eq!(Newtypes.edits(&file, &source).len(), 2);
        // An earlier rule's edit at the struct drops the whole set: no removal without the
        // attribute.
        let out = convert(&source, &[&Replace("struct Id", "struct  Id"), &Newtypes]);
        assert_eq!(out.unwrap(), source.replacen("struct Id", "struct  Id", 1));
    }
}
