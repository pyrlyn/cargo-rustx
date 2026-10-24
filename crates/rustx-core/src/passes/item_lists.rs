//! Item lists (a file, an inline module, a block) for the derive-like passes, which put what
//! they generate right after the item it comes from.
//!
//! Shared because every such pass must reach items at any depth, and in a block must keep the
//! generated items among the statements, before the tail expression.

use std::mem;

use syn::visit_mut::{self, VisitMut};
use syn::{Attribute, Block, Item, ItemMod, Stmt};

use crate::tokens::syntax::conditional_attributes;

/// An element of an item list: an item, or a block statement that may be one.
pub(super) trait Entry: Sized {
    fn item_mut(&mut self) -> Option<&mut Item>;
    fn from_item(item: Item) -> Self;
}

impl Entry for Item {
    fn item_mut(&mut self) -> Option<&mut Item> {
        Some(self)
    }

    fn from_item(item: Item) -> Self {
        item
    }
}

impl Entry for Stmt {
    fn item_mut(&mut self) -> Option<&mut Item> {
        match self {
            Self::Item(item) => Some(item),
            _ => None,
        }
    }

    fn from_item(item: Item) -> Self {
        Self::Item(item)
    }
}

/// A pass that works one item list at a time.
pub(super) trait ExpandList {
    fn expand_list<T: Entry>(&mut self, list: &mut Vec<T>);
}

/// Calls `pass` on every item list of `file`, each list before the lists nested in it.
pub(super) fn for_each_list(file: &mut syn::File, pass: &mut impl ExpandList) {
    Lists(pass).visit_file_mut(file);
}

/// Inserts after each item of `list` what `generate` returns for it.
pub(super) fn insert_after<T: Entry>(
    list: &mut Vec<T>,
    mut generate: impl FnMut(&mut Item) -> Vec<Item>,
) {
    for mut entry in mem::take(list) {
        let generated = entry
            .item_mut()
            .map(|item| {
                let mut generated = generate(item);
                if let Some(attrs) = type_or_impl_attrs(item) {
                    for child in &mut generated {
                        inherit_conditions(attrs, child);
                    }
                }
                generated
            })
            .unwrap_or_default();
        list.push(entry);
        list.extend(generated.into_iter().map(T::from_item));
    }
}

/// Generated siblings exist only when the type they implement exists. Keep any conditions
/// the generator already supplied (field defaults do this), without copying derive/doc attrs.
pub(super) fn inherit_conditions(owner: &[Attribute], generated: &mut Item) {
    let Some(attrs) = type_or_impl_attrs(generated) else {
        return;
    };
    let conditions: Vec<_> = conditional_attributes(owner)
        .into_iter()
        .filter(|attr| !attrs.contains(attr))
        .collect();
    attrs.splice(0..0, conditions);
}

/// The type declarations these derive-like passes accept and the items they generate.
fn type_or_impl_attrs(item: &mut Item) -> Option<&mut Vec<Attribute>> {
    match item {
        Item::Struct(item) => Some(&mut item.attrs),
        Item::Enum(item) => Some(&mut item.attrs),
        Item::Union(item) => Some(&mut item.attrs),
        Item::Impl(item) => Some(&mut item.attrs),
        _ => None,
    }
}

struct Lists<'a, P>(&'a mut P);

impl<P: ExpandList> VisitMut for Lists<'_, P> {
    fn visit_file_mut(&mut self, file: &mut syn::File) {
        self.0.expand_list(&mut file.items);
        visit_mut::visit_file_mut(self, file);
    }

    fn visit_item_mod_mut(&mut self, module: &mut ItemMod) {
        if let Some((_, items)) = &mut module.content {
            self.0.expand_list(items);
        }
        visit_mut::visit_item_mod_mut(self, module);
    }

    fn visit_block_mut(&mut self, block: &mut Block) {
        self.0.expand_list(&mut block.stmts);
        visit_mut::visit_block_mut(self, block);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::{ToTokens, quote};

    /// Puts `struct <Name>Copy;` after every struct.
    struct Copies;

    impl ExpandList for Copies {
        fn expand_list<T: Entry>(&mut self, list: &mut Vec<T>) {
            insert_after(list, |item| match item {
                Item::Struct(item) if !item.ident.to_string().ends_with("Copy") => {
                    let name = quote::format_ident!("{}Copy", item.ident);
                    vec![syn::parse_quote!(struct #name;)]
                }
                _ => Vec::new(),
            });
        }
    }

    #[test]
    fn generated_items_follow_their_item_at_every_depth() {
        let mut file: syn::File = syn::parse2(quote! {
            struct A;
            mod m { struct B; fn f() { struct C; let x = 1; x } }
        })
        .unwrap();
        for_each_list(&mut file, &mut Copies);
        let expected: syn::File = syn::parse2(quote! {
            struct A;
            struct ACopy;
            mod m { struct B; struct BCopy; fn f() { struct C; struct CCopy; let x = 1; x } }
        })
        .unwrap();
        assert_eq!(
            file.into_token_stream().to_string(),
            expected.into_token_stream().to_string()
        );
    }

    #[test]
    fn conditions_follow_generated_siblings_at_every_depth() {
        let mut file: syn::File = syn::parse_quote! {
            #[cfg(any())] #[derive(Clone)] struct A;
            mod m { #[cfg_attr(all(), cfg(any()))] struct B; }
            fn f() { #[cfg(any())] struct C; }
        };
        for_each_list(&mut file, &mut Copies);
        let expected: syn::File = syn::parse_quote! {
            #[cfg(any())] #[derive(Clone)] struct A;
            #[cfg(any())] struct ACopy;
            mod m {
                #[cfg_attr(all(), cfg(any()))] struct B;
                #[cfg_attr(all(), cfg(any()))] struct BCopy;
            }
            fn f() { #[cfg(any())] struct C; #[cfg(any())] struct CCopy; }
        };
        assert_eq!(file, expected);
    }

    #[test]
    fn inherited_conditions_are_not_duplicated() {
        let owner: syn::ItemStruct = syn::parse_quote!(
            #[cfg(any())]
            struct S;
        );
        let mut generated: Item = syn::parse_quote!(
            #[cfg(any())]
            impl S {}
        );
        let expected = generated.clone();
        inherit_conditions(&owner.attrs, &mut generated);
        assert_eq!(generated, expected);
    }
}
