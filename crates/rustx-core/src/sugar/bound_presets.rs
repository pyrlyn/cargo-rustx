//! A trait with only supertraits and its blanket impl back to a `bound` preset: the reverse of
//! `passes/bound_presets.rs` (problem 21; T19.16).
//!
//! A trait followed directly by the blanket impl the pass generates for it,
//!
//! ```text
//! pub trait Key: Clone + Eq + Hash {}
//! impl<__RustxSelf: ?::core::marker::Sized + Clone + Eq + Hash> Key for __RustxSelf {}
//! ```
//!
//! becomes `pub bound Key = Clone + Eq + Hash;`, the bounds copied as written. The trait keeps its
//! attributes and visibility, since the pass keeps them. Only the shape is matched here: a trait
//! with no items, generics, `where` clause or `unsafe`, and an impl of it with one type parameter
//! named as the pass names it, for that parameter, with no items. The rest of the impl (`?Sized`,
//! the bounds with `Self` replaced, `#[cfg]` copies and no other attributes) is left to the
//! driver's check, which drops the edit unless the preset expands to exactly the pair.
//!
//! Refused as well: a trait any other impl in the file may implement, since the preset would
//! hide that the trait is more than its bounds; an impl that is not the next item after its trait
//! (the edit would span the code between); and anything that would drop a comment.

use std::collections::HashMap;

use quote::ToTokens;
use syn::visit::{self, Visit};
use syn::{Block, GenericParam, Item, ItemImpl, ItemMod, ItemTrait, Stmt, Type};

use super::{Edit, ReverseRule, has_comment, span_range};
use crate::passes::bound_presets::PARAM;

pub(crate) struct BoundPresets;

impl ReverseRule for BoundPresets {
    fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit> {
        let mut impls = Impls::default();
        impls.visit_file(file);
        let mut pairs = Pairs {
            source,
            impls: impls.0,
            edits: Vec::new(),
        };
        pairs.visit_file(file);
        pairs.edits
    }
}

/// How many impls in the file implement a trait of each name. Paths are compared by their last
/// segment, so a trait is only converted when nothing else can implement it.
#[derive(Default)]
struct Impls(HashMap<String, usize>);

impl<'ast> Visit<'ast> for Impls {
    fn visit_item_impl(&mut self, imp: &'ast ItemImpl) {
        if let Some((path, _)) = &imp.trait_
            && let Some(last) = path.segments.last()
        {
            *self.0.entry(last.ident.to_string()).or_default() += 1;
        }
        visit::visit_item_impl(self, imp);
    }
}

/// Finds trait and blanket impl pairs in every item list.
struct Pairs<'a> {
    source: &'a str,
    impls: HashMap<String, usize>,
    edits: Vec<Edit>,
}

impl Pairs<'_> {
    /// `None` marks a statement between items, which breaks adjacency.
    fn list<'i>(&mut self, items: impl Iterator<Item = Option<&'i Item>>) {
        let items: Vec<Option<&Item>> = items.collect();
        for pair in items.windows(2) {
            if let [Some(Item::Trait(item)), Some(Item::Impl(imp))] = pair
                && let Some(edit) = self.preset(item, imp)
            {
                self.edits.push(edit);
            }
        }
    }

    /// One edit from the `trait` keyword to the end of the impl, so the trait never becomes a
    /// preset while its impl stays, which would implement it twice.
    fn preset(&self, item: &ItemTrait, imp: &ItemImpl) -> Option<Edit> {
        let bare = item.unsafety.is_none()
            && item.modifiers.auto_token.is_none()
            && item.generics.params.is_empty()
            && item.generics.where_clause.is_none()
            && item.items.is_empty();
        if !bare
            || item.supertraits.is_empty()
            || self.impls.get(&item.ident.to_string()) != Some(&1)
            || !blanket(item, imp)
        {
            return None;
        }
        let bounds = span_range(item.supertraits.to_token_stream())?;
        let start = item.trait_token.span.byte_range().start;
        let end = span_range(imp.to_token_stream())?.end;
        let mut lost = self.source.get(start..end)?.to_owned();
        lost.replace_range(bounds.start - start..bounds.end - start, " ");
        if has_comment(&lost) {
            return None;
        }
        Some(Edit {
            range: start..end,
            text: format!("bound {} = {};", item.ident, self.source.get(bounds)?),
        })
    }
}

/// Whether `imp` is `impl<PARAM: ..> Name for PARAM {}` for the trait `item`.
fn blanket(item: &ItemTrait, imp: &ItemImpl) -> bool {
    let param = match imp.generics.params.first() {
        Some(GenericParam::Type(param)) if imp.generics.params.len() == 1 => param,
        _ => return false,
    };
    let for_param = match &*imp.self_ty {
        Type::Path(ty) => ty.qself.is_none() && ty.path.is_ident(PARAM),
        _ => false,
    };
    param.ident == PARAM
        && for_param
        && imp.unsafety.is_none()
        && imp.modifiers.defaultness.is_none()
        && imp.modifiers.polarity.is_none()
        && imp.generics.where_clause.is_none()
        && imp.items.is_empty()
        && matches!(&imp.trait_, Some((path, _)) if path.is_ident(&item.ident))
}

impl<'ast> Visit<'ast> for Pairs<'_> {
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

#[cfg(test)]
mod tests {
    use super::BoundPresets;
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

    /// The pass's output for `Key`, as rustfmt lays it out.
    const KEY: &str = "trait Key: Clone + Eq + Hash {}\n\
         impl<__RustxSelf: ?::core::marker::Sized + Clone + Eq + Hash> Key for __RustxSelf {}\n";

    const KEY_SUGAR: &str = "bound Key = Clone + Eq + Hash;\n";

    fn unchanged(source: &str) {
        assert_eq!(sugar(source).unwrap(), source);
    }

    #[test]
    fn the_trait_and_its_blanket_impl_become_a_preset() {
        assert_sugar(KEY, KEY_SUGAR);
        let uses = "\nfn count<K: Key>(keys: &[K]) -> usize {\n    keys.len()\n}\n";
        assert_sugar(&format!("{KEY}{uses}"), &format!("{KEY_SUGAR}{uses}"));
    }

    #[test]
    fn a_short_sized_and_a_multi_line_impl_convert() {
        assert_sugar(&KEY.replace("::core::marker::Sized", "Sized"), KEY_SUGAR);
        assert_sugar(
            "trait Key: Clone + Eq + Hash {}\nimpl<\n    __RustxSelf: ?Sized + Clone + Eq + Hash,\n> \
             Key for __RustxSelf\n{\n}\n",
            KEY_SUGAR,
        );
    }

    #[test]
    fn self_in_the_bounds_converts_when_the_impl_names_the_parameter() {
        assert_sugar(
            "trait Num: Copy + Add<Output = Self> {}\n\
             impl<__RustxSelf: ?Sized + Copy + Add<Output = __RustxSelf>> Num for __RustxSelf {}\n",
            "bound Num = Copy + Add<Output = Self>;\n",
        );
    }

    #[test]
    fn visibility_attributes_and_comments_outside_the_pair_are_kept() {
        assert_sugar(
            "// Keys.\n/// Docs.\n#[allow(dead_code)]\npub(crate) trait Key: Clone /* why */ + Eq {}\n\
             impl<__RustxSelf: ?Sized + Clone + Eq> Key for __RustxSelf {}\n",
            "// Keys.\n/// Docs.\n#[allow(dead_code)]\npub(crate) bound Key = Clone /* why */ + Eq;\n",
        );
    }

    #[test]
    fn a_cfg_copied_onto_the_impl_converts() {
        assert_sugar(
            "#[cfg(unix)]\ntrait Key: Clone {}\n#[cfg(unix)]\nimpl<__RustxSelf: ?Sized + Clone> Key for __RustxSelf {}\n",
            "#[cfg(unix)]\nbound Key = Clone;\n",
        );
    }

    #[test]
    fn pairs_in_modules_and_fn_bodies_convert() {
        assert_sugar(
            "mod m {\n    pub trait A: Clone {}\n    impl<__RustxSelf: ?Sized + Clone> A for __RustxSelf {}\n}\n\
             fn f() {\n    trait B: Send {}\n    impl<__RustxSelf: ?Sized + Send> B for __RustxSelf {}\n}\n",
            "mod m {\n    pub bound A = Clone;\n}\nfn f() {\n    bound B = Send;\n}\n",
        );
    }

    #[test]
    fn a_trait_the_pass_would_not_write_is_left_alone() {
        // Items, generics, a `where` clause, `unsafe`, no supertraits.
        unchanged(&KEY.replace("Hash {}\n", "Hash { fn f(&self); }\n"));
        unchanged(
            "trait Key<T>: Clone {}\nimpl<__RustxSelf: ?Sized + Clone> Key<u8> for __RustxSelf {}\n",
        );
        unchanged(&KEY.replace("Hash {}\n", "Hash where Self: Copy {}\n"));
        unchanged(&format!("unsafe {KEY}").replace("impl<", "unsafe impl<"));
        unchanged("trait Key {}\nimpl<__RustxSelf: ?Sized> Key for __RustxSelf {}\n");
    }

    #[test]
    fn an_impl_the_pass_would_not_generate_is_left_alone() {
        // Another parameter name, no `?Sized`, other bounds, items, a `where` clause.
        unchanged(&KEY.replace("__RustxSelf", "T"));
        unchanged(&KEY.replace("?::core::marker::Sized + ", ""));
        unchanged(&KEY.replace("+ Hash> Key", "> Key"));
        unchanged(&KEY.replace("__RustxSelf {}", "__RustxSelf { fn f() {} }"));
        unchanged(&KEY.replace(
            "> Key for __RustxSelf",
            "> Key for __RustxSelf where u8: Copy",
        ));
        // An impl for another type, or of another trait.
        unchanged(&KEY.replace("for __RustxSelf", "for Vec<__RustxSelf>"));
        unchanged(
            "trait Key: Clone {}\nimpl<__RustxSelf: ?Sized + Clone> Other for __RustxSelf {}\n",
        );
    }

    #[test]
    fn attributes_the_pass_would_not_write_are_left_alone() {
        // Docs or a lint on the impl, a `cfg` on the trait only, a `cfg` on the impl only.
        unchanged(&KEY.replace("impl<", "/// Docs.\nimpl<"));
        unchanged(&KEY.replace("impl<", "#[allow(unused)]\nimpl<"));
        unchanged(&format!("#[cfg(unix)]\n{KEY}"));
        unchanged(&KEY.replace("impl<", "#[cfg(unix)]\nimpl<"));
    }

    #[test]
    fn a_trait_with_a_second_impl_is_left_alone() {
        unchanged(&format!("{KEY}impl Key for u8 {{}}\n"));
        unchanged(&format!(
            "{KEY}mod m {{\n    impl super::Key for u8 {{}}\n}}\n"
        ));
    }

    #[test]
    fn an_impl_apart_from_its_trait_or_a_comment_it_would_drop_is_left_alone() {
        unchanged(&KEY.replace("{}\nimpl<", "{}\nfn f() {}\nimpl<"));
        unchanged(&KEY.replace("{}\nimpl<", "{}\n// why\nimpl<"));
        unchanged(&KEY.replace("Hash {}", "Hash { /* why */ }"));
        unchanged(&KEY.replace("?::core", "/* why */ ?::core"));
        unchanged(&KEY.replace("Hash {}", "Hash /* why */ {}"));
    }

    #[test]
    fn a_trait_and_its_impl_are_one_edit_kept_or_dropped_whole() {
        let file = syn::parse_file(KEY).unwrap();
        assert_eq!(BoundPresets.edits(&file, KEY).len(), 1);
        // An earlier rule's edit at the same place drops all of it: no preset beside its impl.
        let out = convert(KEY, &[&Replace("trait Key", "trait  Key"), &BoundPresets]);
        assert_eq!(out.unwrap(), KEY.replacen("trait Key", "trait  Key", 1));
        // A failing edit sends the driver through its one-at-a-time fallback: the pair is kept.
        let source = format!("{KEY}fn f() {{ let x = 1; }}\n");
        let out = convert(&source, &[&Replace("let x", "let y"), &BoundPresets]);
        assert_eq!(
            out.unwrap(),
            format!("{KEY_SUGAR}fn f() {{ let x = 1; }}\n")
        );
    }
}
