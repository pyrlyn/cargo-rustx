//! `rustx-cli expand`: desugars a `.rustx` file, or every `sugar!` invocation in a Rust file, into
//! formatted plain Rust.

use std::path::Path;

use proc_macro2::TokenStream;
use quote::ToTokens;
use syn::parse::{Parse, ParseStream, Parser};
use syn::visit_mut::{self, VisitMut};
use syn::{Block, Expr, ImplItem, Item, Macro, Stmt, TraitItem};

use crate::diagnostic::is_call_site;

/// How a source file is read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// The whole file is a `sugar!` body.
    Sugar,
    /// Plain Rust; only `sugar!` invocations in it are desugared.
    Rust,
}

impl Kind {
    /// A `.rustx` file is sugar throughout; any other file is plain Rust.
    #[must_use]
    pub fn of(path: &Path) -> Self {
        if path.extension().is_some_and(|ext| ext == "rustx") {
            Self::Sugar
        } else {
            Self::Rust
        }
    }
}

/// Desugars `source` and pretty-prints the result.
///
/// Non-doc comments are lost: `prettyplease` prints a syntax tree, which has none.
///
/// # Errors
///
/// Returns every desugaring and parse error, combined, with the spans of the offending tokens.
pub fn expand_source(source: &str, kind: Kind) -> syn::Result<String> {
    let mut file = match kind {
        Kind::Sugar => rustx_core::try_expand(syn::parse_str(source)?)?,
        Kind::Rust => syn::parse_file(source)?,
    };
    // A `.rustx` file may still call `sugar!` itself; the compiler would expand those afterwards.
    let mut expander = Expander { errors: None };
    expander.visit_file_mut(&mut file);
    match expander.errors {
        Some(err) => Err(err),
        None => Ok(prettyplease::unparse(&file)),
    }
}

/// Whether `mac` calls the `sugar!` macro: `sugar!` after a `use`, or by its `rustx` path.
#[must_use]
pub fn is_sugar(mac: &Macro) -> bool {
    let segments: Vec<String> = mac
        .path
        .segments
        .iter()
        .map(|s| s.ident.to_string())
        .collect();
    match segments.as_slice() {
        [name] => name == "sugar" && mac.path.leading_colon.is_none(),
        [krate, name] => krate == "rustx" && name == "sugar",
        _ => false,
    }
}

fn many<T: Parse>(input: ParseStream) -> syn::Result<Vec<T>> {
    let mut out = Vec::new();
    while !input.is_empty() {
        out.push(input.parse()?);
    }
    Ok(out)
}

fn item_sugar(item: &Item) -> Option<&Macro> {
    // `macro_rules! sugar` defines a macro instead of calling one.
    match item {
        Item::Macro(m) if m.ident.is_none() => Some(&m.mac),
        _ => None,
    }
}

fn impl_item_sugar(item: &ImplItem) -> Option<&Macro> {
    match item {
        ImplItem::Macro(m) => Some(&m.mac),
        _ => None,
    }
}

fn trait_item_sugar(item: &TraitItem) -> Option<&Macro> {
    match item {
        TraitItem::Macro(m) => Some(&m.mac),
        _ => None,
    }
}

fn stmt_sugar(stmt: &Stmt) -> Option<&Macro> {
    match stmt {
        Stmt::Macro(m) => Some(&m.mac),
        _ => None,
    }
}

struct Expander {
    errors: Option<syn::Error>,
}

impl Expander {
    fn report(&mut self, err: syn::Error) {
        match &mut self.errors {
            Some(errors) => errors.combine(err),
            None => self.errors = Some(err),
        }
    }

    /// Replaces each `sugar!` invocation in `list` by the elements it expands to, as the compiler
    /// would in that position. A failed invocation stays, so the rest of the file still expands.
    fn splice<T>(
        &mut self,
        list: &mut Vec<T>,
        sugar: fn(&T) -> Option<&Macro>,
        parse: fn(ParseStream) -> syn::Result<Vec<T>>,
    ) {
        for element in std::mem::take(list) {
            let expanded = match sugar(&element) {
                Some(mac) if is_sugar(mac) => self.expand(mac, parse),
                _ => None,
            };
            match expanded {
                Some(mut elements) => {
                    // Expansion output may itself call `sugar!` at this level.
                    self.splice(&mut elements, sugar, parse);
                    list.extend(elements);
                }
                None => list.push(element),
            }
        }
    }

    fn expand<T>(
        &mut self,
        mac: &Macro,
        parse: fn(ParseStream) -> syn::Result<Vec<T>>,
    ) -> Option<Vec<T>> {
        let tokens: TokenStream = match rustx_core::try_expand(mac.tokens.clone()) {
            Ok(file) => file.into_token_stream(),
            Err(err) => {
                self.report(at_invocation(err, mac));
                return None;
            }
        };
        match parse.parse2(tokens) {
            Ok(elements) => Some(elements),
            Err(err) => {
                self.report(at_invocation(err, mac));
                None
            }
        }
    }
}

/// Moves end-of-input errors, which carry no position, onto the invocation's closing delimiter.
fn at_invocation(err: syn::Error, mac: &Macro) -> syn::Error {
    let close = mac.delimiter.span().close();
    err.into_iter()
        .map(|e| {
            if is_call_site(e.span()) {
                syn::Error::new(close, e.to_string())
            } else {
                e
            }
        })
        .reduce(|mut all, e| {
            all.combine(e);
            all
        })
        .unwrap_or(syn::Error::new(close, "`sugar!` failed to expand"))
}

impl VisitMut for Expander {
    fn visit_file_mut(&mut self, file: &mut syn::File) {
        self.splice(&mut file.items, item_sugar, many::<Item>);
        visit_mut::visit_file_mut(self, file);
    }

    fn visit_item_mod_mut(&mut self, module: &mut syn::ItemMod) {
        if let Some((_, items)) = &mut module.content {
            self.splice(items, item_sugar, many::<Item>);
        }
        visit_mut::visit_item_mod_mut(self, module);
    }

    fn visit_item_impl_mut(&mut self, block: &mut syn::ItemImpl) {
        self.splice(&mut block.items, impl_item_sugar, many::<ImplItem>);
        visit_mut::visit_item_impl_mut(self, block);
    }

    fn visit_item_trait_mut(&mut self, block: &mut syn::ItemTrait) {
        self.splice(&mut block.items, trait_item_sugar, many::<TraitItem>);
        visit_mut::visit_item_trait_mut(self, block);
    }

    fn visit_block_mut(&mut self, block: &mut Block) {
        self.splice(&mut block.stmts, stmt_sugar, Block::parse_within);
        visit_mut::visit_block_mut(self, block);
    }

    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        if let Expr::Macro(m) = expr
            && is_sugar(&m.mac)
        {
            // The compiler would parse the expansion, a list of items, as an expression and fail.
            self.report(syn::Error::new_spanned(
                &m.mac.path,
                "`sugar!` expands to items, so it cannot be used as an expression",
            ));
        }
        visit_mut::visit_expr_mut(self, expr);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rust(source: &str) -> String {
        expand_source(source, Kind::Rust).unwrap()
    }

    fn rust_err(source: &str) -> Vec<String> {
        let err = expand_source(source, Kind::Rust).unwrap_err();
        err.into_iter()
            .map(|e| {
                let start = e.span().start();
                format!("{}:{}: {e}", start.line, start.column)
            })
            .collect()
    }

    fn plain(source: &str) -> String {
        prettyplease::unparse(&syn::parse_file(source).unwrap())
    }

    #[test]
    fn kind_follows_the_rustx_extension() {
        assert_eq!(Kind::of(Path::new("a/b.rustx")), Kind::Sugar);
        assert_eq!(Kind::of(Path::new("a/b.rs")), Kind::Rust);
        assert_eq!(Kind::of(Path::new("rustx")), Kind::Rust);
    }

    #[test]
    fn a_rustx_source_is_desugared_as_a_whole() {
        let out = expand_source("fn f() { var x = 1; x += 1; }", Kind::Sugar).unwrap();
        assert_eq!(out, plain("fn f() { let mut x = 1; x += 1; }"));
    }

    #[test]
    fn rust_without_sugar_is_only_reformatted() {
        let source = "macro_rules! sugar { () => {} }\nfn f() { println!(\"{}\", 1); other::sugar! { var x = 1; } }";
        assert_eq!(rust(source), plain(source));
    }

    #[test]
    fn every_sugar_path_form_is_expanded() {
        let out = rust(
            "sugar! { fn a() { var x = 1; } }
             rustx::sugar! { fn b() { var x = 1; } }
             ::rustx::sugar!(fn c() { var x = 1; });",
        );
        assert_eq!(
            out,
            plain("fn a() { let mut x = 1; } fn b() { let mut x = 1; } fn c() { let mut x = 1; }")
        );
    }

    #[test]
    fn sugar_is_expanded_in_modules_impls_traits_and_nested_blocks() {
        let out = rust(
            "mod m { sugar! { fn a() { var x = 1; } } }
             impl S { sugar! { fn b(&self) { var x = 1; } } }
             trait T { sugar! { fn c() { var x = 1; } } }
             fn f() { let g = || { if true { sugar! { fn d() { var x = 1; } } } }; }",
        );
        let expected = plain(
            "mod m { fn a() { let mut x = 1; } }
             impl S { fn b(&self) { let mut x = 1; } }
             trait T { fn c() { let mut x = 1; } }
             fn f() { let g = || { if true { fn d() { let mut x = 1; } } }; }",
        );
        assert_eq!(out, expected);
    }

    #[test]
    fn sugar_inside_an_expansion_is_expanded_too() {
        let out = rust(
            "sugar! { sugar! { fn a() { var x = 1; } } fn b() { sugar! { fn c() { var y = 2; } } } }",
        );
        assert_eq!(
            out,
            plain("fn a() { let mut x = 1; } fn b() { fn c() { let mut y = 2; } }")
        );
    }

    #[test]
    fn a_desugar_error_keeps_the_position_of_the_sugar_token() {
        let errors = rust_err("fn ok() {}\nsugar! {\n    fn f() -> Vec<~> { Vec::new() }\n}\n");
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].starts_with("3:18: expected a trait after `~`"),
            "{errors:?}"
        );
    }

    #[test]
    fn an_end_of_input_error_points_at_the_closing_delimiter() {
        let errors = rust_err("sugar! {\n    fn f() ->\n}\n");
        assert_eq!(errors.len(), 1);
        assert!(
            errors[0].starts_with("3:0: unexpected end of input"),
            "{errors:?}"
        );
    }

    #[test]
    fn errors_from_every_invocation_are_reported() {
        let errors = rust_err("sugar! { fn a() -> ~ {} }\nfn f() { sugar! { fn b() -> ~ {} } }\n");
        assert_eq!(errors.len(), 2, "{errors:?}");
        assert!(errors[0].starts_with("1:19:"), "{errors:?}");
        assert!(errors[1].starts_with("2:28:"), "{errors:?}");
    }

    #[test]
    fn sugar_in_expression_position_is_an_error() {
        let errors = rust_err("fn f() { let x = sugar! { fn g() {} }; }");
        assert_eq!(
            errors,
            ["1:17: `sugar!` expands to items, so it cannot be used as an expression"]
        );
    }

    #[test]
    fn a_rust_syntax_error_is_reported_with_its_position() {
        let errors = rust_err("fn f() {\n    let = 1;\n}\n");
        assert_eq!(errors.len(), 1);
        assert!(errors[0].starts_with("2:8:"), "{errors:?}");
    }
}
