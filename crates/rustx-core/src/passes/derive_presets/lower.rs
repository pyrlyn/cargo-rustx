//! AST half: resolves the markers into presets and expands them in every `#[derive(..)]`.

use std::collections::{HashMap, HashSet};

use proc_macro2::{Ident, TokenStream};
use quote::{ToTokens, quote};
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::visit_mut::{self, VisitMut};
use syn::{Attribute, ItemEnum, ItemStruct, ItemUnion, Meta, Path, Token};

use super::{MARKER, Preset};
use crate::tokens::syntax::{combine, take_attr};

pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    let mut errors = None;
    let presets = collect(file, &mut errors);
    let table = flatten_all(&presets, &mut errors);
    if let Some(errors) = errors {
        return Err(errors);
    }
    if !table.is_empty() {
        Expander { table }.visit_file_mut(file);
    }
    Ok(())
}

/// Takes every marker off the file; a name declared twice keeps its first declaration.
fn collect(file: &mut syn::File, errors: &mut Option<syn::Error>) -> Vec<Preset> {
    let mut presets: Vec<Preset> = Vec::new();
    while let Some(marker) = take_attr(&mut file.attrs, MARKER) {
        match marker.parse_args::<Preset>() {
            Ok(preset) if presets.iter().any(|seen| seen.name == preset.name) => combine(
                errors,
                syn::Error::new(
                    preset.name.span(),
                    format!("`derive` preset `{}` is declared twice", preset.name),
                ),
            ),
            Ok(preset) => presets.push(preset),
            Err(err) => combine(errors, err),
        }
    }
    presets
}

/// Every preset's list with the presets it uses expanded and duplicates removed.
fn flatten_all(presets: &[Preset], errors: &mut Option<syn::Error>) -> HashMap<Ident, Vec<Path>> {
    let mut done = HashMap::new();
    for preset in presets {
        flatten(preset, presets, &mut Vec::new(), &mut done, errors);
    }
    done
}

fn flatten<'a>(
    preset: &'a Preset,
    all: &'a [Preset],
    stack: &mut Vec<&'a Ident>,
    done: &mut HashMap<Ident, Vec<Path>>,
    errors: &mut Option<syn::Error>,
) -> Vec<Path> {
    if let Some(list) = done.get(&preset.name) {
        return list.clone();
    }
    stack.push(&preset.name);
    let mut list = Vec::new();
    for path in &preset.derives {
        let used = path
            .get_ident()
            .and_then(|name| all.iter().find(|other| other.name == *name));
        match used {
            Some(used) if stack.contains(&&used.name) => {
                let cycle: Vec<String> = stack
                    .iter()
                    .skip_while(|name| **name != &used.name)
                    .chain([&&used.name])
                    .map(ToString::to_string)
                    .collect();
                combine(
                    errors,
                    syn::Error::new_spanned(
                        path,
                        format!("`derive` presets form a cycle: {}", cycle.join(" -> ")),
                    ),
                );
            }
            Some(used) => list.extend(flatten(used, all, stack, done, errors)),
            None => list.push(path.clone()),
        }
    }
    stack.pop();
    dedup(&mut list);
    done.insert(preset.name.clone(), list.clone());
    list
}

/// Keeps the first of every path, comparing what was written.
fn dedup(list: &mut Vec<Path>) {
    let mut seen = HashSet::new();
    list.retain(|path| seen.insert(path.to_token_stream().to_string()));
}

struct Expander {
    table: HashMap<Ident, Vec<Path>>,
}

impl Expander {
    fn attributes(&self, attrs: &mut [Attribute]) {
        for attr in attrs {
            let Meta::List(list) = &mut attr.meta else {
                continue;
            };
            if list.path.is_ident("derive") {
                if let Some(tokens) = self.derive_list(&list.tokens) {
                    list.tokens = tokens;
                }
            } else if list.path.is_ident("cfg_attr") {
                self.cfg_attr(&mut list.tokens);
            }
        }
    }

    /// `cond, derive(..), other(..)`: the derive lists after the condition.
    fn cfg_attr(&self, tokens: &mut TokenStream) {
        let Ok(mut metas) = Punctuated::<Meta, Token![,]>::parse_terminated.parse2(tokens.clone())
        else {
            return;
        };
        let mut changed = false;
        for meta in metas.iter_mut().skip(1) {
            if let Meta::List(list) = meta
                && list.path.is_ident("derive")
                && let Some(expanded) = self.derive_list(&list.tokens)
            {
                list.tokens = expanded;
                changed = true;
            }
        }
        if changed {
            *tokens = metas.into_token_stream();
        }
    }

    /// The expanded list, or `None` when it uses no preset (or is not a list of paths), so
    /// everything else stays exactly as written.
    fn derive_list(&self, tokens: &TokenStream) -> Option<TokenStream> {
        let written = Punctuated::<Path, Token![,]>::parse_terminated
            .parse2(tokens.clone())
            .ok()?;
        let preset_of = |path: &Path| path.get_ident().and_then(|name| self.table.get(name));
        if !written.iter().any(|path| preset_of(path).is_some()) {
            return None;
        }
        let mut list: Vec<Path> = written
            .iter()
            .flat_map(|path| {
                preset_of(path)
                    .cloned()
                    .unwrap_or_else(|| vec![path.clone()])
            })
            .collect();
        dedup(&mut list);
        Some(quote! { #(#list),* })
    }
}

impl VisitMut for Expander {
    fn visit_item_struct_mut(&mut self, item: &mut ItemStruct) {
        self.attributes(&mut item.attrs);
        visit_mut::visit_item_struct_mut(self, item);
    }

    fn visit_item_enum_mut(&mut self, item: &mut ItemEnum) {
        self.attributes(&mut item.attrs);
        visit_mut::visit_item_enum_mut(self, item);
    }

    fn visit_item_union_mut(&mut self, item: &mut ItemUnion) {
        self.attributes(&mut item.attrs);
        visit_mut::visit_item_union_mut(self, item);
    }
}
