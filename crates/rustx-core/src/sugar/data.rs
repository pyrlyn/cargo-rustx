//! A struct followed by its constructor, getters, setters and builder back to `#[data(..)]`: the
//! reverse of `passes/data.rs` (problem 08, constructors and getters; T19.20).
//!
//! A struct with named fields followed directly by exactly the items `#[data]` generates for one
//! flag set,
//!
//! ```text
//! pub struct User { id: u64, name: String }
//! #[allow(dead_code)]
//! impl User {
//!     pub fn new(id: u64, name: String) -> Self { Self { id, name } }
//!     pub fn id(&self) -> u64 { self.id }
//!     pub fn name(&self) -> &str { &self.name }
//!     pub fn set_id(&mut self, id: u64) { self.id = id; }
//!     pub fn set_name(&mut self, name: String) { self.name = name; }
//! }
//! ```
//!
//! becomes the struct with `#[data(setters)]` and nothing after it. Every flag set is tried, from
//! none to `setters, with, builder`; the rule expands the struct with each and keeps the one that
//! gives back the struct and the items after it token for token, so nothing about the methods is
//! matched here. Anything the pass would not write stays: a doc comment or an attribute on a
//! method, another method, another order, another visibility, another impl header for the
//! generics, and a builder that differs from the pass's.
//!
//! Not recovered: field defaults (a `new` that leaves a field out), which plain Rust cannot write
//! on the struct; and `#[borrows]` structs, whose lifetime is plain Rust once expanded.
//!
//! Refused as well: items that are not next to each other (the removal would span the code
//! between) and a comment the removal would drop.

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::{Fields, Item, ItemStruct, Visibility};

use super::item_lists::for_each_list;
use super::std_paths::StdPaths;
use super::{Edit, ReverseRule, apply, has_comment, span_range};

pub(crate) struct Data;

impl ReverseRule for Data {
    fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit> {
        self.edit_groups(file, source).concat()
    }

    fn edit_groups(&self, file: &syn::File, source: &str) -> Vec<Vec<Edit>> {
        let paths = StdPaths::of(file);
        let mut groups = Vec::new();
        for_each_list(file, |items| {
            for (at, item) in items.iter().enumerate() {
                if let Some(Item::Struct(item)) = item
                    && let Some(group) = unit(source, &paths, item, &items[at + 1..])
                {
                    groups.push(group);
                }
            }
        });
        groups
    }
}

/// The flags of a flag set, in the order the attribute lists them.
const FLAGS: [&str; 3] = ["setters", "with", "builder"];

/// The attribute for every flag set the pass supports, fewest flags first.
fn attributes() -> impl Iterator<Item = String> {
    (0..1 << FLAGS.len()).map(|set: usize| {
        let flags: Vec<&str> = FLAGS
            .iter()
            .enumerate()
            .filter(|(bit, _)| set & (1 << bit) != 0)
            .map(|(_, flag)| *flag)
            .collect();
        if flags.is_empty() {
            "#[data]".to_owned()
        } else {
            format!("#[data({})]", flags.join(", "))
        }
    })
}

/// The edits adding the attribute to `item` and removing what it generates from `after`, one set:
/// the attribute without the removal would generate the methods twice, the removal alone loses
/// them.
fn unit(
    source: &str,
    paths: &StdPaths,
    item: &ItemStruct,
    after: &[Option<&Item>],
) -> Option<Vec<Edit>> {
    if !matches!(item.fields, Fields::Named(_))
        || !matches!(after.first(), Some(Some(Item::Impl(_))))
    {
        return None;
    }
    let range = span_range(item.to_token_stream())?;
    // After the struct's attributes, so it reads as one of them and its doc comment stays first.
    let at = match &item.vis {
        Visibility::Inherited => item.struct_token.span.byte_range().start,
        vis => span_range(vis.to_token_stream())?.start,
    };
    let line = source.get(..at)?.rfind('\n').map_or(0, |end| end + 1);
    let indent = source.get(line..at)?;
    let separator = if indent.trim().is_empty() {
        format!("\n{indent}")
    } else {
        " ".to_owned()
    };

    attributes().find_map(|attr| {
        let insert = Edit {
            range: at..at,
            text: format!("{attr}{separator}"),
        };
        let sugar = apply(
            source.get(range.clone())?,
            &[Edit {
                range: at - range.start..at - range.start,
                text: insert.text.clone(),
            }],
        );
        let expanded = crate::try_expand(sugar.parse::<TokenStream>().ok()?).ok()?;
        let generated = after.get(..expanded.items.len().checked_sub(1)?)?;
        let generated: Vec<&Item> = generated.iter().copied().collect::<Option<_>>()?;
        if paths.normalise(expanded.into_token_stream())
            != paths.normalise(quote!(#item #(#generated)*))
        {
            return None;
        }
        let end = span_range(generated.last()?.to_token_stream())?.end;
        if has_comment(source.get(range.end..end)?) {
            return None;
        }
        Some(vec![
            insert,
            Edit {
                range: range.end..end,
                text: String::new(),
            },
        ])
    })
}

#[cfg(test)]
mod tests {
    use quote::ToTokens;

    use super::Data;
    use crate::sugar::tests::assert_sugar;
    use crate::sugar::{ReverseRule, sugar};

    const USER: &str = "#[derive(Debug)]\npub struct User {\n    id: u64,\n    name: String,\n}\n";

    /// `struct` with `attr` as the pass expands it, laid out as rustfmt would, then a fn.
    fn generated(attr: &str, struct_: &str) -> String {
        let expanded = crate::try_expand(format!("{attr}\n{struct_}").parse().unwrap()).unwrap();
        format!("{}\nfn after() {{}}\n", prettyplease::unparse(&expanded))
    }

    /// The pass's output for `User` with `attr` converts back to `User` with `attr`.
    fn converts(attr: &str) {
        let sugar = USER.replace("pub struct", &format!("{attr}\npub struct"));
        assert_sugar(
            &generated(attr, USER),
            &format!("{sugar}\nfn after() {{}}\n"),
        );
    }

    fn unchanged(source: &str) {
        assert_eq!(sugar(source).unwrap(), source);
    }

    #[test]
    fn new_and_getters_alone_become_data() {
        converts("#[data]");
    }

    #[test]
    fn setters_become_data_setters() {
        converts("#[data(setters)]");
    }

    #[test]
    fn withs_become_data_with() {
        converts("#[data(with)]");
    }

    #[test]
    fn a_builder_becomes_data_builder() {
        converts("#[data(builder)]");
    }

    #[test]
    fn setters_and_withs_become_data_setters_with() {
        converts("#[data(setters, with)]");
    }

    #[test]
    fn setters_and_a_builder_become_data_setters_builder() {
        converts("#[data(setters, builder)]");
    }

    #[test]
    fn withs_and_a_builder_become_data_with_builder() {
        converts("#[data(with, builder)]");
    }

    #[test]
    fn every_flag_becomes_data_setters_with_builder() {
        converts("#[data(setters, with, builder)]");
    }

    #[test]
    fn a_builder_written_with_short_std_names_converts() {
        assert_sugar(
            "/// A user.\npub struct User {\n    id: u64,\n}\n\n#[allow(dead_code)]\nimpl User {\n    \
             pub fn new(id: u64) -> Self {\n        Self { id }\n    }\n    \
             pub fn id(&self) -> u64 {\n        self.id\n    }\n    \
             pub fn builder() -> UserBuilder {\n        UserBuilder { id: None }\n    }\n}\n\n\
             pub struct UserBuilder {\n    id: Option<u64>,\n}\n\n#[allow(dead_code)]\n\
             impl UserBuilder {\n    pub fn id(mut self, id: u64) -> Self {\n        \
             self.id = Some(id);\n        self\n    }\n    \
             pub fn build(self) -> Result<User, &'static str> {\n        Ok(User {\n            \
             id: self.id.ok_or(\"`User` builder is missing `id`\")?,\n        })\n    }\n}\n",
            "/// A user.\n#[data(builder)]\npub struct User {\n    id: u64,\n}\n",
        );
    }

    #[test]
    fn generic_structs_and_structs_in_modules_and_fn_bodies_convert() {
        let generic = "struct Pair<'a, T: Clone> where T: Copy { key: &'a str, value: T }";
        let expanded = crate::try_expand(format!("#[data] {generic}").parse().unwrap()).unwrap();
        let written = expanded.items[0].to_token_stream().to_string();
        let plain = expanded.into_token_stream().to_string();
        assert_sugar(
            &format!("mod m {{ {plain} }}\nfn f() {{ {plain} let x = 1; }}\n"),
            &format!("mod m {{ #[data] {written} }}\nfn f() {{ #[data] {written} let x = 1; }}\n"),
        );
    }

    #[test]
    fn the_attribute_and_the_removal_are_one_set() {
        let source = generated("#[data(setters)]", USER);
        let file = syn::parse_file(&source).unwrap();
        let groups = Data.edit_groups(&file, &source);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }

    /// The `#[data(setters)]` output for `User` with `from` replaced by `to` stays as written.
    fn changed_stays(from: &str, to: &str) {
        let source = generated("#[data(setters)]", USER);
        assert!(source.contains(from), "{from} in {source}");
        unchanged(&source.replacen(from, to, 1));
    }

    #[test]
    fn a_doc_comment_or_an_attribute_on_a_method_stays() {
        changed_stays("    pub fn id(", "    /// The id.\n    pub fn id(");
        changed_stays("    pub fn id(", "    #[inline]\n    pub fn id(");
    }

    #[test]
    fn another_method_stays() {
        changed_stays(
            "    pub fn id(",
            "    pub fn other(&self) -> u8 {\n        1\n    }\n    pub fn id(",
        );
    }

    #[test]
    fn methods_in_another_order_stay() {
        let source = generated("#[data]", USER);
        let getters = "    pub fn id(&self) -> u64 {\n        self.id\n    }\n    \
                       pub fn name(&self) -> &str {\n        &self.name\n    }\n";
        assert!(source.contains(getters), "{source}");
        unchanged(&source.replace(
            getters,
            "    pub fn name(&self) -> &str {\n        &self.name\n    }\n    \
             pub fn id(&self) -> u64 {\n        self.id\n    }\n",
        ));
    }

    #[test]
    fn another_visibility_or_a_missing_allow_stays() {
        changed_stays("    pub fn new(", "    fn new(");
        changed_stays("    pub fn set_id(", "    pub(crate) fn set_id(");
        changed_stays("#[allow(dead_code)]\n", "");
    }

    #[test]
    fn another_getter_or_body_stays() {
        changed_stays(
            "pub fn name(&self) -> &str",
            "pub fn name(&self) -> &String",
        );
        changed_stays("Self { id, name }", "Self { name, id }");
    }

    #[test]
    fn an_impl_header_the_pass_would_not_write_stays() {
        unchanged(
            "struct S<T: Clone> {\n    t: T,\n}\n#[allow(dead_code)]\nimpl<T> S<T>\nwhere\n    \
             T: Clone,\n{\n    pub fn new(t: T) -> Self {\n        Self { t }\n    }\n    \
             pub fn t(&self) -> &T {\n        &self.t\n    }\n}\n",
        );
    }

    #[test]
    fn a_builder_that_differs_from_the_passs_stays() {
        // Other rules may still rewrite inside it (`build` becomes a `throws` fn).
        let not_data = |source: String| {
            let out = sugar(&source).unwrap();
            assert!(
                !out.contains("#[data") && out.contains("impl User {"),
                "{out}"
            );
        };
        let source = generated("#[data(builder)]", USER);
        not_data(source.replacen(
            "pub struct UserBuilder",
            "#[derive(Debug)]\npub struct UserBuilder",
            1,
        ));
        not_data(source.replacen("builder is missing `name`", "builder needs `name`", 1));
    }

    #[test]
    fn items_apart_or_a_comment_the_removal_would_drop_stay() {
        let source = generated("#[data]", USER);
        unchanged(&source.replacen(
            "#[allow(dead_code)]",
            "fn between() {}\n#[allow(dead_code)]",
            1,
        ));
        unchanged(&source.replacen("#[allow(dead_code)]", "// why\n#[allow(dead_code)]", 1));
        unchanged(&source.replacen("        self.id\n", "        // why\n        self.id\n", 1));
    }

    #[test]
    fn a_struct_without_named_fields_or_an_impl_after_it_stays() {
        unchanged(
            "struct T(u8);\nimpl T {\n    pub fn new(t: u8) -> Self {\n        Self(t)\n    }\n}\n",
        );
        unchanged("struct S {\n    a: u8,\n}\nfn f() {}\n");
    }

    #[test]
    fn comments_inside_the_struct_survive() {
        let struct_ = "pub struct User {\n    // Never zero.\n    id: u64,\n}\n";
        let source = generated("#[data]", "pub struct User {\n    id: u64,\n}\n").replacen(
            "pub struct User {\n    id: u64,\n}\n",
            struct_,
            1,
        );
        assert_sugar(&source, &format!("#[data]\n{struct_}\nfn after() {{}}\n"));
    }
}
