//! Recognising cargo-rustx attributes in the AST before macro expansion.
//!
//! Before expansion nothing is resolved, so a rustx attribute macro is recognised by how it is
//! spelled: `#[rustx::data]` or `#[::rustx::data]`, or a bare `#[data]` whose name a `use` of
//! `rustx` brought in earlier in the same file. Inside an item that `#[sugared]` covers, the bare
//! names (`#[data]`, `#[display("..")]`, `#[from]`, ...) are rustx's by definition.

use rustc_ast::{self as ast, AttrKind, Attribute, UseTreeKind};
use rustc_data_structures::fx::FxHashMap;
use rustc_span::{Span, Symbol, kw};

/// The attribute macros the `rustx` crate exports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Macro {
    Data,
    Display,
    Delegate,
    Newtype,
    Sugared,
}

impl Macro {
    pub const ALL: [Self; 5] = [Self::Data, Self::Display, Self::Delegate, Self::Newtype, Self::Sugared];

    pub fn name(self) -> &'static str {
        match self {
            Self::Data => "data",
            Self::Display => "display",
            Self::Delegate => "delegate",
            Self::Newtype => "newtype",
            Self::Sugared => "sugared",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|m| m.name() == name)
    }
}

/// The names `use rustx::..` brought into the file, and whether a glob did.
#[derive(Default)]
pub struct Imports {
    names: FxHashMap<Symbol, Macro>,
    glob: bool,
}

impl Imports {
    /// Records the rustx macros a `use` item imports.
    pub fn record(&mut self, item: &ast::Item) {
        if let ast::ItemKind::Use(tree) = &item.kind {
            self.record_tree(tree, &[]);
        }
    }

    fn record_tree(&mut self, tree: &ast::UseTree, outer: &[Symbol]) {
        let mut path = outer.to_vec();
        path.extend(segments(&tree.prefix));
        match &tree.kind {
            UseTreeKind::Simple(rename) => {
                if let [krate, name] = path.as_slice()
                    && krate.as_str() == "rustx"
                    && let Some(mac) = Macro::from_name(name.as_str())
                {
                    let local = rename.map_or(*name, |ident| ident.name);
                    self.names.insert(local, mac);
                }
            }
            UseTreeKind::Nested { items, .. } => {
                for nested in items {
                    self.record_tree(&nested.tree, &path);
                }
            }
            UseTreeKind::Glob(_) => {
                if let [krate] = path.as_slice()
                    && krate.as_str() == "rustx"
                {
                    self.glob = true;
                }
            }
        }
    }

    fn bare(&self, name: Symbol) -> Option<Macro> {
        self.names.get(&name).copied().or_else(|| {
            self.glob
                .then(|| Macro::from_name(name.as_str()))
                .flatten()
        })
    }
}

/// How a rustx attribute macro is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spelling {
    /// `#[rustx::data]`: always the attribute macro, even inside `#[sugared]`.
    Qualified,
    /// `#[data]`, imported with `use rustx::data`.
    Imported,
}

/// The rustx attribute macro `attr` invokes, if it is one.
pub fn rustx_macro(attr: &Attribute, imports: &Imports) -> Option<(Macro, Spelling)> {
    let path = attr_path(attr)?;
    match path.as_slice() {
        [krate, name] if krate.as_str() == "rustx" => {
            Macro::from_name(name.as_str()).map(|mac| (mac, Spelling::Qualified))
        }
        [name] => imports.bare(*name).map(|mac| (mac, Spelling::Imported)),
        _ => None,
    }
}

/// The first rustx attribute macro among `attrs`.
pub fn find_macro(attrs: &[Attribute], imports: &Imports, mac: Macro) -> Option<(&Attribute, Spelling)> {
    attrs.iter().find_map(|attr| {
        rustx_macro(attr, imports)
            .filter(|(found, _)| *found == mac)
            .map(|(_, spelling)| (attr, spelling))
    })
}

/// The path of a normal attribute, without a leading `::`.
pub fn attr_path(attr: &Attribute) -> Option<Vec<Symbol>> {
    match &attr.kind {
        AttrKind::Normal(normal) => Some(segments(&normal.item.path)),
        _ => None,
    }
}

fn segments(path: &ast::Path) -> Vec<Symbol> {
    path.segments
        .iter()
        .map(|segment| segment.ident.name)
        .filter(|name| *name != kw::PathRoot)
        .collect()
}

/// Whether `attr` is the single-segment attribute `name`, as the sugar under `#[sugared]` is
/// written.
pub fn is_bare(attr: &Attribute, name: &str) -> bool {
    attr_path(attr).is_some_and(|path| matches!(path.as_slice(), [only] if only.as_str() == name))
}

pub fn has_bare(attrs: &[Attribute], name: &str) -> bool {
    attrs.iter().any(|attr| is_bare(attr, name))
}

/// The span of the attribute's path, `rustx::data` in `#[rustx::data(builder)]`.
pub fn path_span(attr: &Attribute) -> Option<Span> {
    match &attr.kind {
        AttrKind::Normal(normal) => Some(normal.item.path.span),
        _ => None,
    }
}

/// The traits a `#[derive(..)]` among `attrs` names, by their last path segment, with the span of
/// each path.
pub fn derived(attrs: &[Attribute]) -> Vec<(Symbol, Span)> {
    attrs
        .iter()
        .filter(|attr| is_bare(attr, "derive"))
        .filter_map(Attribute::meta_item_list)
        .flatten()
        .filter_map(|inner| {
            let path = &inner.meta_item()?.path;
            Some((path.segments.last()?.ident.name, path.span))
        })
        .collect()
}
