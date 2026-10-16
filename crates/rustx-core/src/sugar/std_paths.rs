//! Std path spellings the sugar check treats as one (T19.3).
//!
//! Passes emit fully qualified paths (`::std::rc::Rc::new`, `::core::option::Option`), while
//! ordinary code writes `Rc::new` or `Option`. Decided with the creator: the check
//! `expand(sugar(x)) == x` compares paths to a known std item ([`ITEMS`]) by the item they name,
//! however they are spelled, unless the file gives the name another meaning. This only compares;
//! the user's text is never rewritten to a canonical spelling.
//!
//! # What a name means
//!
//! The first segment of a path is resolved file-wide, which is coarser than Rust's per-module
//! scopes and therefore errs towards "unknown":
//!
//! - the crates `std`, `core` and `alloc`, and with a leading `::` always the crate;
//! - prelude names (`Option`, `Some`, `Vec`, `Into`, ...) by their prelude item;
//! - names a `use` imports from those crates (`use std::rc::Rc;`, `use std::rc;` for `rc::Rc`,
//!   `use std::sync::Arc as Rc;` for `Arc`).
//!
//! A name is unknown, and every path through it is compared as written, when anything in the
//! file declares it otherwise: an item or enum variant, an `extern crate` of another crate, a
//! generic parameter, a binding, a `use` from outside std (`crate::`, another crate, a renamed
//! `super::` import), or a `use` of a std path other than the name's prelude item or other
//! imports. A glob import makes
//! every name unknown, except `use super::*` that stays inside the file, which only re-imports
//! names the file already declares; `#![no_implicit_prelude]` drops the prelude.
//!
//! `std`, `core` and `alloc` re-export the same items, so they are equated, except in a file that
//! says `#![no_std]` or names `alloc`: there `std` may not exist, and an expansion that writes
//! `::std` would not build where the original did. Only paths to an item in [`ITEMS`] are
//! equated, because the crates do differ elsewhere (`core::panic!` and `std::panic!`).
//!
//! Limits: declarations made by a macro call are not seen, nor is the `#![no_std]` of a crate
//! root in another file.

use std::collections::{HashMap, HashSet};

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};
use quote::ToTokens;
use syn::visit::{self, Visit};
use syn::{ForeignItem, GenericParam, Item, ItemExternCrate, ItemUse, PatIdent, UseTree, Variant};

/// The std items whose spellings are equated, by their path from the crate that defines them,
/// and whether the prelude brings their name into scope.
const ITEMS: &[(&str, bool)] = &[
    ("core::option::Option", true),
    ("core::option::Option::Some", true),
    ("core::option::Option::None", true),
    ("core::result::Result", true),
    ("core::result::Result::Ok", true),
    ("core::result::Result::Err", true),
    ("core::convert::Into", true),
    ("core::convert::From", true),
    ("core::convert::AsRef", true),
    ("core::convert::TryFrom", true),
    ("core::convert::TryInto", true),
    ("core::default::Default", true),
    ("core::clone::Clone", true),
    ("core::iter::Iterator", true),
    ("core::iter::IntoIterator", true),
    ("core::iter::DoubleEndedIterator", true),
    ("core::marker::Send", true),
    ("core::marker::Sync", true),
    ("core::marker::Sized", true),
    ("core::cell::Cell", false),
    ("core::cell::RefCell", false),
    ("core::fmt::Debug", false),
    ("core::fmt::Display", false),
    ("core::fmt::Formatter", false),
    ("core::fmt::Result", false),
    ("core::hash::Hash", false),
    ("core::hash::Hasher", false),
    ("core::ops::Deref", false),
    ("core::str::FromStr", false),
    ("alloc::borrow::ToOwned", true),
    ("alloc::boxed::Box", true),
    ("alloc::string::String", true),
    ("alloc::string::ToString", true),
    ("alloc::vec::Vec", true),
    ("alloc::rc::Rc", false),
    ("alloc::rc::Weak", false),
    ("alloc::sync::Arc", false),
    ("std::collections::HashMap", false),
    ("std::collections::HashSet", false),
    ("std::io::Result", false),
    ("std::sync::Mutex", false),
    ("std::sync::RwLock", false),
];

const CRATES: &[&str] = &["std", "core", "alloc"];

/// Marks a punctuation leaf glued to the next token; no identifier or literal can end in it.
const JOINT: char = '\u{1}';

/// Keywords after which `::` starts a path rather than continuing one.
const BEFORE_A_PATH: &[&str] = &[
    "as", "box", "break", "const", "dyn", "else", "for", "if", "impl", "in", "let", "match",
    "move", "mut", "pub", "ref", "return", "static", "unsafe", "use", "where", "while", "yield",
];

/// What the first segments of paths mean in one file.
pub(super) struct StdPaths {
    /// The std path, crate first, of every name that has exactly one.
    roots: HashMap<String, Vec<String>>,
    /// Whether `std`, `core` and `alloc` count as one crate.
    one_crate: bool,
}

impl StdPaths {
    pub(super) fn of(file: &syn::File) -> Self {
        let mut found = Declarations::default();
        found.visit_file(file);
        let one_crate = !found.no_std && !names_alloc(file.to_token_stream());

        let mut other: HashSet<String> = HashSet::new();
        let mut imports: Vec<(String, Vec<String>, bool)> = Vec::new();
        for (name, meaning) in found.names {
            match meaning {
                Meaning::Other => {
                    other.insert(name);
                }
                Meaning::Std { path, leading } => imports.push((name, path, leading)),
                Meaning::Same => {}
            }
        }
        let mut candidates: HashMap<String, Vec<Vec<String>>> = HashMap::new();
        if !found.no_prelude {
            for (path, _) in ITEMS.iter().filter(|(_, prelude)| *prelude) {
                let path: Vec<String> = path.split("::").map(str::to_owned).collect();
                candidates
                    .entry(path[path.len() - 1].clone())
                    .or_default()
                    .push(path);
            }
        }
        for (name, path, leading) in imports {
            // `use std::..` without `::` names a local `std` when the file declares one.
            if !leading && other.contains(&path[0]) {
                other.insert(name);
            } else {
                candidates.entry(name).or_default().push(path);
            }
        }

        let mut roots: HashMap<String, Vec<String>> = CRATES
            .iter()
            .filter(|krate| !other.contains(**krate))
            .map(|krate| ((*krate).to_owned(), vec![(*krate).to_owned()]))
            .collect();
        if !found.glob {
            let key = |path: &[String]| path[usize::from(one_crate)..].to_vec();
            for (name, paths) in candidates {
                if !other.contains(&name) && paths.iter().all(|path| key(path) == key(&paths[0])) {
                    roots.entry(name).or_insert_with(|| paths[0].clone());
                }
            }
        }
        Self { roots, one_crate }
    }

    /// The tokens of `tokens` as the sugar check compares them: without trailing commas that
    /// mean nothing ([`super::commas`]), and with every path to a known std item replaced by one
    /// canonical token, so that two spellings of that path compare equal. Delimiters and spacing
    /// are kept, since they take part in what the tokens mean.
    pub(super) fn normalise(&self, tokens: TokenStream) -> Vec<String> {
        let tokens = super::without_trailing_commas(tokens, super::MacroTokens::Kept);
        let mut out = Vec::new();
        self.flatten(&tokens.into_iter().collect::<Vec<_>>(), &mut out);
        out
    }

    fn flatten(&self, seq: &[TokenTree], out: &mut Vec<String>) {
        let mut at = 0;
        while at < seq.len() {
            if let Some((end, canonical)) = self.path_at(seq, at) {
                // The `:` of the spelling glued the token before it to the path; the canonical
                // form has no `:`, so that spacing says nothing.
                if let Some(last) = out.last_mut()
                    && last.chars().count() == 2
                    && last.ends_with(JOINT)
                {
                    last.pop();
                }
                out.push(canonical);
                at = end;
                continue;
            }
            match &seq[at] {
                TokenTree::Group(group) => {
                    let (open, close) = match group.delimiter() {
                        Delimiter::Parenthesis => ("(", ")"),
                        Delimiter::Brace => ("{", "}"),
                        Delimiter::Bracket => ("[", "]"),
                        Delimiter::None => ("", ""),
                    };
                    out.push(open.to_owned());
                    self.flatten(&group.stream().into_iter().collect::<Vec<_>>(), out);
                    out.push(close.to_owned());
                }
                TokenTree::Punct(punct) => {
                    let mut leaf = punct.as_char().to_string();
                    if punct.spacing() == Spacing::Joint {
                        leaf.push(JOINT);
                    }
                    out.push(leaf);
                }
                leaf => out.push(leaf.to_string()),
            }
            at += 1;
        }
    }

    /// The end and canonical form of a path to a known std item that starts at `at`.
    fn path_at(&self, seq: &[TokenTree], at: usize) -> Option<(usize, String)> {
        let leading = is_path_sep(seq, at);
        if !starts_a_path(seq, at, leading) {
            return None;
        }
        let mut segments = Vec::new();
        let mut end = at + if leading { 2 } else { 0 };
        while let Some(TokenTree::Ident(segment)) = seq.get(end) {
            segments.push(segment.to_string());
            end += 1;
            if !(is_path_sep(seq, end) && matches!(seq.get(end + 2), Some(TokenTree::Ident(_)))) {
                break;
            }
            end += 2;
        }
        let first = segments.first()?;
        let mut path = if leading {
            CRATES
                .contains(&first.as_str())
                .then(|| vec![first.clone()])?
        } else {
            self.roots.get(first)?.clone()
        };
        path.extend(segments.into_iter().skip(1));
        let below = &path[1..];
        let known = ITEMS.iter().any(|(item, _)| {
            let item: Vec<&str> = item.split("::").skip(1).collect();
            below.len() >= item.len() && below.iter().zip(&item).all(|(a, b)| a == b)
        });
        let krate = if self.one_crate { "std" } else { &path[0] };
        known.then(|| (end, format!("::{krate}::{}", below.join("::"))))
    }
}

fn is_path_sep(seq: &[TokenTree], at: usize) -> bool {
    matches!(
        (seq.get(at), seq.get(at + 1)),
        (Some(TokenTree::Punct(a)), Some(TokenTree::Punct(b)))
            if a.as_char() == ':' && a.spacing() == Spacing::Joint && b.as_char() == ':'
    )
}

/// Whether a path starts at `at` rather than continuing one (`a::b`, `Vec::<u8>::new`) or
/// naming a field, a method, a label or a macro variable.
fn starts_a_path(seq: &[TokenTree], at: usize, leading: bool) -> bool {
    let Some(before) = at.checked_sub(1).map(|p| &seq[p]) else {
        return true;
    };
    if leading {
        return match before {
            TokenTree::Ident(ident) => BEFORE_A_PATH.iter().any(|kw| ident == kw),
            // `->` and `=>` end in `>` as well as generic arguments do.
            TokenTree::Punct(punct) if punct.as_char() == '>' => at.checked_sub(2).is_some_and(
                |p| matches!(&seq[p], TokenTree::Punct(arrow) if matches!(arrow.as_char(), '-' | '=') && arrow.spacing() == Spacing::Joint),
            ),
            _ => true,
        };
    }
    let continues = at >= 2 && is_path_sep(seq, at - 2);
    let names_a_member = matches!(before, TokenTree::Punct(punct) if matches!(punct.as_char(), '.' | '$' | '\'' | '#'));
    !continues && !names_a_member
}

/// Whether the tokens name the `alloc` crate, which a crate without `std` uses.
fn names_alloc(tokens: TokenStream) -> bool {
    let seq: Vec<TokenTree> = tokens.into_iter().collect();
    seq.iter().enumerate().any(|(at, tree)| match tree {
        TokenTree::Ident(ident) => ident == "alloc" && is_path_sep(&seq, at + 1),
        TokenTree::Group(group) => names_alloc(group.stream()),
        _ => false,
    })
}

enum Meaning {
    /// A `use` of this std path; `leading` when it starts with `::`.
    Std {
        path: Vec<String>,
        leading: bool,
    },
    /// A re-import of the name from a module of this file, which adds no meaning.
    Same,
    Other,
}

#[derive(Default)]
struct Declarations {
    names: Vec<(String, Meaning)>,
    /// Inline modules around the visited code, which `super` can reach without leaving the file.
    depth: usize,
    glob: bool,
    no_std: bool,
    no_prelude: bool,
}

impl Declarations {
    fn declare(&mut self, name: &impl ToString, meaning: Meaning) {
        self.names.push((name.to_string(), meaning));
    }

    fn use_tree(&mut self, leading: bool, prefix: &mut Vec<String>, tree: &UseTree) {
        match tree {
            UseTree::Path(path) => {
                prefix.push(path.ident.to_string());
                self.use_tree(leading, prefix, &path.tree);
                prefix.pop();
            }
            UseTree::Name(name) => self.import(leading, prefix, &name.ident.to_string(), None),
            UseTree::Rename(rename) => {
                let alias = rename.rename.to_string();
                self.import(leading, prefix, &rename.ident.to_string(), Some(alias));
            }
            UseTree::Glob(_) => {
                let in_file = !leading
                    && !prefix.is_empty()
                    && prefix.len() <= self.depth
                    && prefix.iter().all(|segment| segment == "super");
                self.glob |= !in_file;
            }
            UseTree::Group(group) => {
                for tree in &group.items {
                    self.use_tree(leading, prefix, tree);
                }
            }
        }
    }

    fn import(&mut self, leading: bool, prefix: &[String], ident: &str, alias: Option<String>) {
        let mut path = prefix.to_vec();
        if ident != "self" {
            path.push(ident.to_owned());
        }
        let Some(last) = path.last().cloned() else {
            return;
        };
        let name = alias.unwrap_or_else(|| last.clone());
        if name == "_" {
            return;
        }
        let supers = path
            .iter()
            .take_while(|segment| *segment == "super")
            .count();
        let meaning = match path[0].as_str() {
            first if CRATES.contains(&first) => Meaning::Std { path, leading },
            "self" | "super" if !leading && name == last && supers <= self.depth => Meaning::Same,
            _ => Meaning::Other,
        };
        self.declare(&name, meaning);
    }

    fn extern_crate(&mut self, item: &ItemExternCrate) {
        let name = item.rename.as_ref().map_or(&item.ident, |(_, alias)| alias);
        let meaning = if CRATES.iter().any(|krate| item.ident == krate) {
            Meaning::Std {
                path: vec![item.ident.to_string()],
                leading: true,
            }
        } else {
            Meaning::Other
        };
        self.declare(name, meaning);
    }
}

impl<'ast> Visit<'ast> for Declarations {
    fn visit_item(&mut self, item: &'ast Item) {
        let name = match item {
            Item::Const(item) => Some(&item.ident),
            Item::Enum(item) => Some(&item.ident),
            Item::Fn(item) => Some(&item.sig.ident),
            Item::Mod(item) => Some(&item.ident),
            Item::Static(item) => Some(&item.ident),
            Item::Struct(item) => Some(&item.ident),
            Item::Trait(item) => Some(&item.ident),
            Item::TraitAlias(item) => Some(&item.ident),
            Item::Type(item) => Some(&item.ident),
            Item::Union(item) => Some(&item.ident),
            Item::ExternCrate(item) => {
                self.extern_crate(item);
                None
            }
            _ => None,
        };
        if let Some(name) = name {
            self.declare(name, Meaning::Other);
        }
        let inline_mod = matches!(item, Item::Mod(item) if item.content.is_some());
        self.depth += usize::from(inline_mod);
        visit::visit_item(self, item);
        self.depth -= usize::from(inline_mod);
    }

    fn visit_item_use(&mut self, item: &'ast ItemUse) {
        self.use_tree(item.leading_colon.is_some(), &mut Vec::new(), &item.tree);
    }

    fn visit_foreign_item(&mut self, item: &'ast ForeignItem) {
        match item {
            ForeignItem::Fn(item) => self.declare(&item.sig.ident, Meaning::Other),
            ForeignItem::Static(item) => self.declare(&item.ident, Meaning::Other),
            ForeignItem::Type(item) => self.declare(&item.ident, Meaning::Other),
            _ => {}
        }
        visit::visit_foreign_item(self, item);
    }

    // A variant is not an item, but `use self::E::Ok;` brings it in as one, and that import
    // alone looks like a re-import from this file.
    fn visit_variant(&mut self, variant: &'ast Variant) {
        self.declare(&variant.ident, Meaning::Other);
        visit::visit_variant(self, variant);
    }

    fn visit_generic_param(&mut self, param: &'ast GenericParam) {
        match param {
            GenericParam::Type(param) => self.declare(&param.ident, Meaning::Other),
            GenericParam::Const(param) => self.declare(&param.ident, Meaning::Other),
            GenericParam::Lifetime(_) => {}
        }
        visit::visit_generic_param(self, param);
    }

    fn visit_pat_ident(&mut self, pat: &'ast PatIdent) {
        // `syn` reads the pattern `None` as a binding, but while `None` is the prelude's it can
        // only be that variant, so it is not a declaration.
        if pat.ident != "None" {
            self.declare(&pat.ident, Meaning::Other);
        }
        visit::visit_pat_ident(self, pat);
    }

    fn visit_attribute(&mut self, attr: &'ast syn::Attribute) {
        let path = attr.path();
        self.no_std |= path.is_ident("no_std");
        self.no_prelude |= path.is_ident("no_implicit_prelude");
        visit::visit_attribute(self, attr);
    }
}

#[cfg(test)]
mod tests {
    use super::StdPaths;

    /// Whether `a` and `b`, in a file declaring `decls`, compare equal.
    fn same(decls: &str, a: &str, b: &str) -> bool {
        let file = syn::parse_file(decls).unwrap();
        let paths = StdPaths::of(&file);
        paths.normalise(a.parse().unwrap()) == paths.normalise(b.parse().unwrap())
    }

    #[test]
    fn short_and_fully_qualified_std_names_are_equal() {
        let decls = "use std::rc::Rc; use std::cell::RefCell;";
        assert!(same(
            decls,
            "Rc::new(RefCell::new(1))",
            "::std::rc::Rc::new(::std::cell::RefCell::new(1))"
        ));
        assert!(same(
            "",
            "fn f(x: Option<u8>) -> Result<Vec<String>, ()> { Ok(Some(x).into_iter().collect()) }",
            "fn f(x: ::core::option::Option<u8>) -> ::core::result::Result<::std::vec::Vec<\
             ::std::string::String>, ()> { ::core::result::Result::Ok(::core::option::Option::\
             Some(x).into_iter().collect()) }"
        ));
        assert!(same("", "None", "::core::option::Option::None"));
        assert!(same(
            "",
            "Default::default()",
            "::core::default::Default::default()"
        ));
    }

    #[test]
    fn partially_qualified_paths_are_equal_when_their_module_comes_from_std() {
        let decls = "use std::rc; use core::cell::{self, RefCell as Cell2};";
        assert!(same(
            decls,
            "rc::Rc::new(cell::RefCell::new(1))",
            "::std::rc::Rc::new(::std::cell::RefCell::new(1))"
        ));
        assert!(same(decls, "std::rc::Rc", "::alloc::rc::Rc"));
        assert!(same(decls, "Cell2::new", "::core::cell::RefCell::new"));
        assert!(same(
            "use std::sync::Arc as Rc;",
            "Rc::new",
            "::std::sync::Arc::new"
        ));
        // Without `use std::rc`, `rc` is a crate or module of the user's.
        assert!(!same("", "rc::Rc", "::std::rc::Rc"));
    }

    #[test]
    fn a_use_an_item_or_a_generic_parameter_of_that_name_turns_equality_off() {
        let full = "::std::rc::Rc::new";
        assert!(!same("use std::rc::Rc; use my::Rc;", "Rc::new", full));
        assert!(!same("use std::sync::Arc as Rc;", "Rc::new", full));
        assert!(!same("use std::rc::Rc; struct Rc;", "Rc::new", full));
        assert!(!same(
            "use std::rc::Rc; mod m { pub struct Rc; }",
            "Rc::new",
            full
        ));
        assert!(!same("use std::rc::Rc; fn f<Rc>() {}", "Rc::new", full));
        assert!(!same(
            "use std::rc::Rc; fn f() { let Rc = 1; }",
            "Rc::new",
            full
        ));
        assert!(!same(
            "enum E { Some(u8) } use E::*;",
            "Some",
            "::core::option::Option::Some"
        ));
        assert!(!same(
            "use std::io::Result;",
            "Result",
            "::core::result::Result"
        ));
        assert!(!same("mod std {} use std::rc::Rc;", "Rc", "::std::rc::Rc"));
        assert!(!same(
            "#![no_implicit_prelude]",
            "Option",
            "::core::option::Option"
        ));
        // Other names keep their equality.
        assert!(same("struct Rc;", "Option", "::core::option::Option"));
    }

    #[test]
    fn re_imports_inside_the_file_keep_equality_and_other_globs_do_not() {
        let decls = "use std::rc::Rc; mod m { use super::*; mod n { use super::super::Rc; } }";
        assert!(same(decls, "Rc::new", "::std::rc::Rc::new"));
        assert!(!same(
            "use std::rc::Rc; use super::*;",
            "Rc::new",
            "::std::rc::Rc::new"
        ));
        assert!(!same(
            "use std::rc::Rc; use other::*;",
            "Rc::new",
            "::std::rc::Rc::new"
        ));
        let matches_none = "fn f(o: Option<u8>) { match o { None => {} _ => {} } }";
        assert!(same(matches_none, "None", "::core::option::Option::None"));
    }

    #[test]
    fn an_enum_variant_of_the_file_turns_equality_off_for_its_name() {
        let ok = "::core::result::Result::Ok(1)";
        let outcome = "enum Outcome { Ok(u8), Fail }";
        assert!(!same(
            &format!("{outcome} use self::Outcome::Ok;"),
            "Ok(1)",
            ok
        ));
        assert!(!same(
            &format!("mod m {{ pub {outcome} }} use m::Outcome::Ok;"),
            "Ok(1)",
            ok
        ));
        assert!(!same(&format!("{outcome} use Outcome::*;"), "Ok(1)", ok));
        assert!(same(outcome, "Fail", "Fail"));
    }

    #[test]
    fn a_non_std_path_with_the_same_last_segment_is_not_equal() {
        let decls = "use std::rc::Rc;";
        assert!(!same(decls, "my::Rc::new", "::std::rc::Rc::new"));
        assert!(!same(decls, "self::Rc::new", "::std::rc::Rc::new"));
        assert!(!same(decls, "Foo::<u8>::Rc", "::std::rc::Rc"));
        assert!(!same("", "::my::option::Option", "::core::option::Option"));
        assert!(!same("", "std::panic!()", "core::panic!()"));
    }

    #[test]
    fn without_std_the_crates_stay_apart() {
        assert!(!same(
            "#![no_std] use alloc::rc::Rc;",
            "Rc::new",
            "::std::rc::Rc::new"
        ));
        assert!(!same("use alloc::rc::Rc;", "Rc::new", "::std::rc::Rc::new"));
        assert!(same(
            "use alloc::rc::Rc;",
            "Rc::new",
            "::alloc::rc::Rc::new"
        ));
    }
}
