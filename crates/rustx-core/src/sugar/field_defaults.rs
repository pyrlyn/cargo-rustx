//! `impl Default` back to field defaults, and `..Default::default()` back to `..`: the reverse
//! of `passes/field_defaults.rs` (problem 17, fields; T19.12).
//!
//! A struct with named fields followed directly by the impl the pass generates for it,
//!
//! ```text
//! struct Config { host: String, port: u16, tls: bool }
//! impl Default for Config {
//!     fn default() -> Self { Self { host: "a".into(), port: 8080, tls: Default::default() } }
//! }
//! ```
//!
//! with every field set once in a `Self { .. }` literal and nothing else in the impl, becomes the
//! struct with `= e` on its fields and no impl. A field set to `Default::default()` (any spelling
//! `std_paths` equates) gets no default; one written as `String` set to `Into::into("lit")` gets
//! `= "lit"`, which the pass converts back. Generics, the `where` clause and `#[cfg]` attributes
//! are not matched here: the rule expands the struct it would write and keeps it only when that
//! gives back the struct and the impl, so an impl the pass would not generate (other statements,
//! a field missing, a struct deriving `Default`, a `Default` that is not std's) is left alone.
//!
//! A struct expression `Config { port: 1, ..Default::default() }` for a struct converted this
//! way becomes `Config { port: 1, .. }`, when the file declares only one item of that name.
//!
//! Refused as well: an impl that is not the next item after its struct (the edit would span the
//! code between), and anything that would drop a comment (in the impl outside the values that
//! move, between the struct and the impl, or inside the replaced `..` rest).

use std::collections::HashMap;

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::visit::{self, Visit};
use syn::{
    Block, Expr, ExprAssign, ExprStruct, Fields, ImplItem, Item, ItemImpl, ItemMod, ItemStruct,
    Member, Stmt, UseName, UseRename,
};

use super::rc_values::bounds;
use super::std_paths::StdPaths;
use super::{Edit, ReverseRule, apply};
use crate::tokens::syntax::{is_str_literal, is_string};

pub(crate) struct FieldDefaults;

impl ReverseRule for FieldDefaults {
    fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit> {
        let paths = StdPaths::of(file);
        let mut units = Units {
            source,
            paths: &paths,
            default_trait: paths.normalise(quote!(::core::default::Default)),
            default_call: paths.normalise(quote!(::core::default::Default::default())),
            into: paths.normalise(quote!(::core::convert::Into::into)),
            converted: Vec::new(),
            edits: Vec::new(),
        };
        units.visit_file(file);

        let mut declared = Declared::default();
        declared.visit_file(file);
        let mut rests = Rests {
            source,
            paths: &paths,
            default_call: units.default_call,
            names: units
                .converted
                .into_iter()
                .filter(|name| declared.0.get(name) == Some(&1))
                .collect(),
            edits: units.edits,
        };
        rests.visit_file(file);
        rests.edits
    }
}

/// Finds struct and impl pairs in every item list.
struct Units<'a> {
    source: &'a str,
    paths: &'a StdPaths,
    default_trait: Vec<String>,
    default_call: Vec<String>,
    into: Vec<String>,
    /// Names of the structs converted, for their `..` rests.
    converted: Vec<String>,
    edits: Vec<Edit>,
}

impl Units<'_> {
    /// `None` marks a statement between items, which breaks adjacency.
    fn list<'i>(&mut self, items: impl Iterator<Item = Option<&'i Item>>) {
        let items: Vec<Option<&Item>> = items.collect();
        for pair in items.windows(2) {
            if let [Some(Item::Struct(item)), Some(Item::Impl(imp))] = pair
                && let Some(edit) = self.unit(item, imp)
            {
                self.edits.push(edit);
                self.converted.push(item.ident.to_string());
            }
        }
    }

    /// One edit adding the defaults to the struct and removing its impl, so the driver keeps or
    /// drops both together; partial, either half would change the tokens. It covers the bytes
    /// from the first default to the end of the impl, nothing outside the pair.
    // Moves onto an edit group (one edit per default, one for the impl) when T19.11 lands.
    fn unit(&self, item: &ItemStruct, imp: &ItemImpl) -> Option<Edit> {
        let Fields::Named(fields) = &item.fields else {
            return None;
        };
        let values = self.values(imp)?;
        if values.len() != fields.named.len() {
            return None;
        }

        let (struct_start, struct_end) = span_of(item.to_token_stream())?;
        let (impl_start, impl_end) = span_of(imp.to_token_stream())?;
        if !self.source.get(struct_end..impl_start)?.trim().is_empty() {
            return None;
        }
        // Byte ranges of the impl that move into the struct, and where each one goes.
        let mut moved = Vec::new();
        for field in &fields.named {
            let ident = field.ident.as_ref()?;
            let (_, value) = values.iter().find(|(name, _)| *name == ident)?;
            if self.paths.normalise(value.to_token_stream()) == self.default_call {
                continue;
            }
            let value = match value {
                Expr::Call(call)
                    if is_string(&field.ty)
                        && call.args.len() == 1
                        && is_str_literal(&call.args[0])
                        && self.paths.normalise(call.func.to_token_stream()) == self.into =>
                {
                    &call.args[0]
                }
                value => value,
            };
            let (_, at) = span_of(field.ty.to_token_stream())?;
            moved.push((span_range(value.to_token_stream())?, at));
        }
        let first = moved.first()?.1;

        let mut lost = self.source.get(impl_start..impl_end)?.to_owned();
        let mut kept: Vec<_> = moved.iter().map(|(range, _)| range).collect();
        kept.sort_by_key(|range| range.start);
        for range in kept.into_iter().rev() {
            lost.replace_range(range.start - impl_start..range.end - impl_start, " ");
        }
        if has_comment(&lost) {
            return None;
        }

        let mut inner = moved
            .iter()
            .map(|(range, at)| {
                Some(Edit {
                    range: at - first..at - first,
                    text: format!(" = {}", self.source.get(range.clone())?),
                })
            })
            .collect::<Option<Vec<Edit>>>()?;
        let sugar_struct = format!(
            "{}{}",
            self.source.get(struct_start..first)?,
            apply(self.source.get(first..struct_end)?, &inner)
        );
        if !self.reads_back(&sugar_struct, item, imp) {
            return None;
        }
        inner.push(Edit {
            range: struct_end - first..impl_end - first,
            text: String::new(),
        });
        Some(Edit {
            range: first..impl_end,
            text: apply(self.source.get(first..impl_end)?, &inner),
        })
    }

    /// The field values of an impl that is `Default`'s with one `fn default() -> Self` building
    /// one `Self { .. }` literal of named fields, and nothing else.
    fn values<'i>(&self, imp: &'i ItemImpl) -> Option<Vec<(&'i syn::Ident, &'i Expr)>> {
        // Modifiers such as `!` are not matched: the expansion check refuses them.
        let (path, _) = imp.trait_.as_ref()?;
        if self.paths.normalise(path.to_token_stream()) != self.default_trait {
            return None;
        }
        let [ImplItem::Fn(func)] = imp.items.as_slice() else {
            return None;
        };
        let [Stmt::Expr(Expr::Struct(literal), None)] = func.block.stmts.as_slice() else {
            return None;
        };
        if func.sig.ident != "default" || !literal.path.is_ident("Self") || literal.rest.is_some() {
            return None;
        }
        literal
            .fields
            .iter()
            .map(|field| match &field.member {
                Member::Named(ident) => Some((ident, &field.expr)),
                Member::Unnamed(_) => None,
            })
            .collect()
    }

    /// Whether `sugar`, the struct with its defaults, expands to exactly `item` and `imp`.
    fn reads_back(&self, sugar: &str, item: &ItemStruct, imp: &ItemImpl) -> bool {
        sugar
            .parse::<TokenStream>()
            .ok()
            .and_then(|tokens| crate::try_expand(tokens).ok())
            .is_some_and(|file| {
                self.paths.normalise(file.into_token_stream())
                    == self.paths.normalise(quote!(#item #imp))
            })
    }
}

impl<'ast> Visit<'ast> for Units<'_> {
    fn visit_file(&mut self, file: &'ast syn::File) {
        self.list(file.items.iter().map(Some));
        visit::visit_file(self, file);
    }

    fn visit_item_mod(&mut self, module: &'ast ItemMod) {
        if let Some((_, items)) = &module.content {
            self.list(items.iter().map(Some));
        }
        visit::visit_item_mod(self, module);
    }

    fn visit_block(&mut self, block: &'ast Block) {
        self.list(block.stmts.iter().map(|stmt| match stmt {
            Stmt::Item(item) => Some(item),
            _ => None,
        }));
        visit::visit_block(self, block);
    }
}

/// How many items and imports in the file declare each name, so a struct expression's single
/// name is only taken for a converted struct when nothing else can be meant.
#[derive(Default)]
struct Declared(HashMap<String, usize>);

impl Declared {
    fn add(&mut self, ident: &syn::Ident) {
        *self.0.entry(ident.to_string()).or_default() += 1;
    }
}

impl<'ast> Visit<'ast> for Declared {
    fn visit_item(&mut self, item: &'ast Item) {
        match item {
            Item::Struct(item) => self.add(&item.ident),
            Item::Enum(item) => self.add(&item.ident),
            Item::Union(item) => self.add(&item.ident),
            Item::Type(item) => self.add(&item.ident),
            _ => {}
        }
        visit::visit_item(self, item);
    }

    fn visit_use_name(&mut self, name: &'ast UseName) {
        self.add(&name.ident);
    }

    fn visit_use_rename(&mut self, rename: &'ast UseRename) {
        self.add(&rename.rename);
    }
}

/// Finds `..Default::default()` rests of the converted structs.
// Separate edits, since a struct's expressions sit anywhere in the file and one edit spanning
// them would swallow every other rule's edits in between. A rest is only edited for a struct
// whose unit the rule itself checked expands back, and a bare `..` expands to the same rest for
// any struct, so a rest kept without its unit still reads back; with T19.11 it joins the group.
struct Rests<'a> {
    source: &'a str,
    paths: &'a StdPaths,
    default_call: Vec<String>,
    names: Vec<String>,
    edits: Vec<Edit>,
}

impl<'ast> Visit<'ast> for Rests<'_> {
    fn visit_expr_struct(&mut self, expr: &'ast ExprStruct) {
        if let (Some(dots), Some(rest)) = (&expr.dot2_token, &expr.rest)
            && expr.qself.is_none()
            && let Some(ident) = expr.path.get_ident()
            && self.names.iter().any(|name| ident == name)
            && self.paths.normalise(rest.to_token_stream()) == self.default_call
            && let Some(rest) = span_range(rest.to_token_stream())
        {
            let range = dots.spans[1].byte_range().end..rest.end;
            if self
                .source
                .get(range.clone())
                .is_some_and(|gap| !has_comment(gap))
            {
                self.edits.push(Edit {
                    range,
                    text: String::new(),
                });
            }
        }
        visit::visit_expr_struct(self, expr);
    }

    // The left side of `=` is a pattern, which the pass leaves alone.
    fn visit_expr_assign(&mut self, assign: &'ast ExprAssign) {
        self.visit_expr(&assign.right);
    }
}

/// The byte range of `tokens` in the source, from its first token to its last.
fn span_range(tokens: TokenStream) -> Option<std::ops::Range<usize>> {
    let (first, last) = bounds(tokens)?;
    Some(first.byte_range().start..last.byte_range().end)
}

fn span_of(tokens: TokenStream) -> Option<(usize, usize)> {
    span_range(tokens).map(|range| (range.start, range.end))
}

fn has_comment(text: &str) -> bool {
    text.contains("//") || text.contains("/*")
}

#[cfg(test)]
mod tests {
    use super::FieldDefaults;
    use crate::sugar::tests::assert_sugar;
    use crate::sugar::{Edit, ReverseRule, convert, sugar};

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

    /// The pass's output for `Config`, as rustfmt lays it out with short std names.
    const CONFIG: &str = "struct Config {\n    host: String,\n    port: u16,\n    tls: bool,\n}\n\n\
         impl Default for Config {\n    fn default() -> Self {\n        Self {\n            \
         host: Into::into(\"127.0.0.1\"),\n            port: 8080,\n            \
         tls: Default::default(),\n        }\n    }\n}\n";

    const CONFIG_SUGAR: &str = "struct Config {\n    host: String = \"127.0.0.1\",\n    \
         port: u16 = 8080,\n    tls: bool,\n}\n";

    fn unchanged(source: &str) {
        assert_eq!(sugar(source).unwrap(), source);
    }

    #[test]
    fn the_generated_impl_becomes_field_defaults_and_rests_become_bare() {
        let uses = "\nfn f() -> Config {\n    let c = Config { port: 1, ..Default::default() };\n    \
                    Config { ..::std::default::Default::default() }\n}\n";
        assert_sugar(
            &format!("{CONFIG}{uses}"),
            &format!(
                "{CONFIG_SUGAR}{}",
                uses.replace("..Default::default()", "..")
                    .replace("..::std::default::Default::default()", "..")
            ),
        );
        // Any other value moves as written, `"..".into()` included: the pass copies it back.
        assert_sugar(
            &CONFIG.replace("Into::into(\"127.0.0.1\")", "\"127.0.0.1\".into()"),
            &CONFIG_SUGAR.replace("\"127.0.0.1\"", "\"127.0.0.1\".into()"),
        );
    }

    #[test]
    fn generics_where_clauses_cfgs_and_nested_structs_convert() {
        assert_sugar(
            "#[cfg(unix)]\nstruct S<T: Clone = u8> where T: Copy { #[cfg(test)] t: T, n: u8 }\n\
             #[cfg(unix)]\nimpl<T: Clone> ::core::default::Default for S<T> where T: Copy {\n    \
             fn default() -> Self { Self { #[cfg(test)] t: T::MIN, n: ::core::default::Default::default(), } }\n}\n\
             fn f() {\n    struct B { b: Vec<u8> }\n    impl Default for B { fn default() -> Self { Self { b: vec![1], } } }\n}\n",
            "#[cfg(unix)]\nstruct S<T: Clone = u8> where T: Copy { #[cfg(test)] t: T = T::MIN, n: u8 }\n\
             fn f() {\n    struct B { b: Vec<u8> = vec![1] }\n}\n",
        );
    }

    #[test]
    fn an_impl_the_pass_would_not_generate_is_left_alone() {
        // Another statement in `default`.
        unchanged(&CONFIG.replace("        Self {", "        let _ = 1;\n        Self {"));
        // A field not set, and every field left to `Default::default()`.
        unchanged(&CONFIG.replace(
            "            tls: Default::default(),\n",
            "            ..Default::default()\n",
        ));
        unchanged(
            "struct A { a: u8 }\nimpl Default for A { fn default() -> Self { Self { a: Default::default(), } } }\n",
        );
        // Another item in the impl, or a body not building `Self`.
        unchanged(
            "struct A { a: u8 }\nimpl Default for A { fn default() -> Self { Self { a: 1, } } const X: u8 = 1; }\n",
        );
        unchanged(
            "struct A { a: u8 }\nimpl Default for A { fn default() -> Self { A { a: 1, } } }\n",
        );
    }

    #[test]
    fn a_literal_without_the_trailing_comma_the_pass_writes_is_left_alone() {
        // rustfmt's one-line `Self { a: 1 }`; T19.11 is to make the check ignore that comma.
        unchanged(
            "struct A { a: u8 }\nimpl Default for A { fn default() -> Self { Self { a: 1 } } }\n",
        );
    }

    #[test]
    fn a_struct_deriving_default_is_left_alone() {
        unchanged(
            "#[derive(Default)]\nstruct A { a: u8 }\nimpl Default for A { fn default() -> Self { Self { a: 1, } } }\n",
        );
    }

    #[test]
    fn a_shadowed_default_is_left_alone() {
        unchanged(
            "trait Default { fn default() -> Self; }\nstruct A { a: u8 }\n\
             impl Default for A { fn default() -> Self { Self { a: 1, } } }\n",
        );
        unchanged(
            "use my::Default;\nstruct A { a: u8 }\n\
             impl Default for A { fn default() -> Self { Self { a: 1, } } }\n",
        );
    }

    #[test]
    fn an_impl_apart_from_its_struct_or_a_comment_it_would_drop_is_left_alone() {
        unchanged(
            "struct A { a: u8 }\nfn f() {}\nimpl Default for A { fn default() -> Self { Self { a: 1, } } }\n",
        );
        unchanged(
            "struct A { a: u8 }\n// why\nimpl Default for A { fn default() -> Self { Self { a: 1, } } }\n",
        );
        unchanged(
            "struct A { a: u8 }\nimpl Default for A { fn default() -> Self { /* why */ Self { a: 1, } } }\n",
        );
        // The rest keeps its comment, while its struct converts.
        assert_sugar(
            "struct A { a: u8 }\nimpl Default for A { fn default() -> Self { Self { a: 1, } } }\n\
             fn f() -> A { A { .. /* why */ Default::default() } }\n",
            "struct A { a: u8 = 1 }\nfn f() -> A { A { .. /* why */ Default::default() } }\n",
        );
        // A comment inside a value moves with it; one after it would be lost.
        unchanged(
            "struct A { a: u8 }\nimpl Default for A { fn default() -> Self { Self { a: 1 /* why */, } } }\n",
        );
        assert_sugar(
            "struct A { a: u8 }\nimpl Default for A { fn default() -> Self { Self { a: 1 + /* why */ 2, } } }\n",
            "struct A { a: u8 = 1 + /* why */ 2 }\n",
        );
    }

    #[test]
    fn rests_of_unconverted_or_ambiguous_structs_stay_as_written() {
        unchanged("struct A { a: u8 }\nfn f() -> A { A { ..Default::default() } }\n");
        assert_sugar(
            "struct A { a: u8 }\nimpl Default for A { fn default() -> Self { Self { a: 1, } } }\n\
             mod m { pub struct A; }\nfn f() -> A { A { ..Default::default() } }\n",
            "struct A { a: u8 = 1 }\nmod m { pub struct A; }\nfn f() -> A { A { ..Default::default() } }\n",
        );
        // A rest the pass does not write stays, while its struct converts.
        assert_sugar(
            "struct A { a: u8 }\nimpl Default for A { fn default() -> Self { Self { a: 1, } } }\n\
             fn f() -> A { A { ..A::default() } }\n",
            "struct A { a: u8 = 1 }\nfn f() -> A { A { ..A::default() } }\n",
        );
    }

    #[test]
    fn a_struct_and_its_impl_are_one_edit_kept_or_dropped_whole() {
        let source = "struct A { a: u8, b: u8 }\n\
                      impl Default for A { fn default() -> Self { Self { a: 1, b: 2, } } }\n\
                      fn f() { let x = 1; }\n";
        let file = syn::parse_file(source).unwrap();
        assert_eq!(FieldDefaults.edits(&file, source).len(), 1);
        // A failing edit sends the driver through its one-at-a-time fallback: the unit is kept.
        let out = convert(source, &[&Replace("let x", "let y"), &FieldDefaults]);
        assert_eq!(
            out.unwrap(),
            "struct A { a: u8 = 1, b: u8 = 2 }\nfn f() { let x = 1; }\n"
        );
        // An earlier edit overlapping the unit drops all of it: no default without the impl gone.
        let out = convert(source, &[&Replace("a: u8,", "a: u8 ,"), &FieldDefaults]);
        assert_eq!(out.unwrap(), source.replacen("a: u8,", "a: u8 ,", 1));
    }
}
