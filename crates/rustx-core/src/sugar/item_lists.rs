//! Item lists (a file, an inline module, a block) for the rules that pair an item with the impls
//! a pass generated right after it.
//!
//! Shared because every such rule must reach lists at any depth, and in a block must see a
//! statement between two items as breaking their adjacency.

use syn::visit::{self, Visit};
use syn::{Block, Item, ItemMod, Stmt};

/// Calls `list` on every item list of `file`, each list before the lists nested in it. `None`
/// marks a statement between the items of a block.
pub(super) fn for_each_list<'ast>(file: &'ast syn::File, list: impl FnMut(&[Option<&'ast Item>])) {
    Lists(list).visit_file(file);
}

struct Lists<F>(F);

impl<'ast, F: FnMut(&[Option<&'ast Item>])> Visit<'ast> for Lists<F> {
    fn visit_file(&mut self, file: &'ast syn::File) {
        (self.0)(&file.items.iter().map(Some).collect::<Vec<_>>());
        visit::visit_file(self, file);
    }

    fn visit_item_mod(&mut self, module: &'ast ItemMod) {
        if let Some((_, items)) = &module.content {
            (self.0)(&items.iter().map(Some).collect::<Vec<_>>());
        }
        visit::visit_item_mod(self, module);
    }

    fn visit_block(&mut self, block: &'ast Block) {
        let items: Vec<Option<&Item>> = block
            .stmts
            .iter()
            .map(|stmt| match stmt {
                Stmt::Item(item) => Some(item),
                _ => None,
            })
            .collect();
        (self.0)(&items);
        visit::visit_block(self, block);
    }
}
