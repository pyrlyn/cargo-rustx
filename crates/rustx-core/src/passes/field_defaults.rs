//! Field defaults `port: u16 = 8080` → `impl Default`, and `S { a: 1, .. }` →
//! `S { a: 1, ..::core::default::Default::default() }` (problem 17, fields).
//!
//! `syn` already parses both forms of RFC 3681: a named field keeps its default in
//! `Field::default`, and a struct expression ending in a bare `..` has `dot2_token` without
//! `rest`. So there is no token half: all the work happens on the AST, where the
//! grammar, not a token heuristic, tells a struct expression from a struct pattern:
//!
//! ```text
//! struct Config { host: String = "127.0.0.1", port: u16 = 8080, tls: bool }
//! let c = Config { port: 3000, .. };
//!
//! struct Config { host: String, port: u16, tls: bool }
//! impl ::core::default::Default for Config {
//!     fn default() -> Self {
//!         Self {
//!             host: ::core::convert::Into::into("127.0.0.1"),
//!             port: 8080,
//!             tls: ::core::default::Default::default(),
//!         }
//!     }
//! }
//! let c = Config { port: 3000, ..::core::default::Default::default() };
//! ```
//!
//! # Rules
//!
//! - **The impl follows the struct** in the same item list (file, module or block), with the
//!   struct's generics, `where` clause and `#[cfg]` attributes; a field's `#[cfg]` goes onto its
//!   value, so configured-out fields take their default with them.
//! - **Fields without a default** take `::core::default::Default::default()`.
//! - **A string literal default on a field written as `String`** (any path ending in a bare
//!   `String`) is converted with `Into::into`, the problem's "item 15" spelling; `Into` keeps it
//!   working in `no_std` crates with `alloc`.
//! - **A bare `..`** in any struct expression is filled with `Default::default()`, whether or not
//!   the struct is declared in the input: valid Rust never has it, and RFC 3681 means exactly
//!   that for structs whose fields all have defaults.
//!
//! # Errors
//!
//! - A struct with field defaults that also derives `Default`: two impls would conflict. The
//!   error points at `Default` in the derive list.
//! - A default on an enum variant or union field: neither can be built from `Default::default()`
//!   with `..`. The error points at the `=`.
//! - A default on a tuple struct field: `syn` rejects it while parsing, at the default.
//!
//! # Limitations
//!
//! - **Macro arguments.** Macro bodies are opaque tokens and may hold patterns (`matches!`), so
//!   `..` is filled only inside the std macros whose arguments are all expressions
//!   ([`EXPRESSION_MACROS`]), and only when the body parses as expressions (`a, b` or
//!   `vec![x; n]`).
//! - **Destructuring assignment.** The left side of `=` is a pattern (`S { a, .. } = s;`), so it
//!   is never filled.
//! - **Generic fields** without a default need `T: Default` on the struct, since the bounds are
//!   copied, not inferred like `#[derive(Default)]` does.

use proc_macro2::{Span, TokenStream};
use quote::{ToTokens, quote, quote_spanned};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit_mut::{self, VisitMut};
use syn::{
    Attribute, Block, Expr, ExprAssign, ExprLit, ExprStruct, Field, Fields, Item, ItemMod,
    ItemStruct, ItemUnion, Lit, Macro, Path, Token, Type, Variant, parse_quote_spanned,
};

use crate::passes::item_lists::{Entry, insert_after};
use crate::passes::macro_args::MacroArgs;
use crate::tokens::syntax::combine;

/// Std macros whose arguments are all expressions, so a struct literal in them is never a
/// pattern.
const EXPRESSION_MACROS: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "dbg",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "eprint",
    "eprintln",
    "format",
    "format_args",
    "panic",
    "print",
    "println",
    "todo",
    "unimplemented",
    "unreachable",
    "vec",
    "write",
    "writeln",
];

pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    let mut lower = Lower {
        errors: None,
        filled: 0,
    };
    lower.visit_file_mut(file);
    lower.errors.map_or(Ok(()), Err)
}

struct Lower {
    errors: Option<syn::Error>,
    /// How many `..` were filled so far, so a macro body is reprinted only when it changed.
    filled: usize,
}

impl Lower {
    fn error(&mut self, span: Span, message: &str) {
        combine(&mut self.errors, syn::Error::new(span, message));
    }

    /// Takes the defaults off `struct`s in `list` and puts their `impl Default` right after them.
    fn expand<T: Entry>(&mut self, list: &mut Vec<T>) {
        insert_after(list, |item| match item {
            Item::Struct(item) => self.default_impl(item).into_iter().collect(),
            _ => Vec::new(),
        });
    }

    fn default_impl(&mut self, item: &mut ItemStruct) -> Option<Item> {
        // Tuple structs never get here with a default: `syn` rejects one while parsing.
        let Fields::Named(fields) = &mut item.fields else {
            return None;
        };
        if fields.named.iter().all(|field| field.default.is_none()) {
            return None;
        }
        let values: Vec<TokenStream> = fields.named.iter_mut().map(field_value).collect();
        if let Some(derive) = derived_default(&item.attrs) {
            self.error(
                derive,
                "`Default` cannot be derived for a struct with field defaults; remove it from \
                 `#[derive(..)]`, the field defaults generate `impl Default`",
            );
            return None;
        }
        let name = &item.ident;
        let cfgs = item.attrs.iter().filter(|attr| is_cfg(attr));
        let (impl_generics, ty_generics, where_clause) = item.generics.split_for_impl();
        let default = quote_spanned! {name.span()=>
            #(#cfgs)*
            impl #impl_generics ::core::default::Default for #name #ty_generics #where_clause {
                fn default() -> Self {
                    Self { #(#values,)* }
                }
            }
        };
        syn::parse2(default)
            .map_err(|err| combine(&mut self.errors, err))
            .ok()
    }

    /// Reports and removes the defaults of fields that cannot have one.
    fn reject<'a>(&mut self, fields: impl IntoIterator<Item = &'a mut Field>, message: &str) {
        for field in fields {
            if let Some((eq, _)) = field.default.take() {
                self.error(eq.span, message);
            }
        }
    }
}

impl VisitMut for Lower {
    fn visit_file_mut(&mut self, file: &mut syn::File) {
        self.expand(&mut file.items);
        visit_mut::visit_file_mut(self, file);
    }

    fn visit_item_mod_mut(&mut self, module: &mut ItemMod) {
        if let Some((_, items)) = &mut module.content {
            self.expand(items);
        }
        visit_mut::visit_item_mod_mut(self, module);
    }

    fn visit_block_mut(&mut self, block: &mut Block) {
        self.expand(&mut block.stmts);
        visit_mut::visit_block_mut(self, block);
    }

    fn visit_variant_mut(&mut self, variant: &mut Variant) {
        self.reject(
            &mut variant.fields,
            "enum variant fields take no defaults; only struct fields do",
        );
        visit_mut::visit_variant_mut(self, variant);
    }

    fn visit_item_union_mut(&mut self, union: &mut ItemUnion) {
        self.reject(
            &mut union.fields.named,
            "union fields take no defaults; only struct fields do",
        );
        visit_mut::visit_item_union_mut(self, union);
    }

    fn visit_expr_struct_mut(&mut self, expr: &mut ExprStruct) {
        if let (Some(dots), None) = (&expr.dot2_token, &expr.rest) {
            let rest = quote_spanned!(dots.spans[0]=> ::core::default::Default::default());
            match syn::parse2(rest) {
                Ok(rest) => {
                    expr.rest = Some(Box::new(rest));
                    self.filled += 1;
                }
                Err(err) => combine(&mut self.errors, err),
            }
        }
        visit_mut::visit_expr_struct_mut(self, expr);
    }

    // The left side of `S { a, .. } = s` is a destructuring pattern, where `..` is the rest.
    fn visit_expr_assign_mut(&mut self, assign: &mut ExprAssign) {
        self.visit_expr_mut(&mut assign.right);
    }

    fn visit_macro_mut(&mut self, mac: &mut Macro) {
        if !mac
            .path
            .segments
            .last()
            .is_some_and(|last| EXPRESSION_MACROS.iter().any(|name| last.ident == name))
        {
            return;
        }
        let Ok(mut args) = mac.parse_body::<MacroArgs>() else {
            return;
        };
        let before = self.filled;
        for arg in args.exprs_mut() {
            self.visit_expr_mut(arg);
        }
        if self.filled != before {
            mac.tokens = args.into_token_stream();
        }
    }
}

/// `name: <default>` for the generated literal, taking the default off the field.
fn field_value(field: &mut Field) -> TokenStream {
    let name = &field.ident;
    let cfgs = field.attrs.iter().filter(|attr| is_cfg(attr));
    let value = field.default.take().map_or_else(
        || quote_spanned!(name.span()=> ::core::default::Default::default()),
        |(_, expr)| default_value(&expr, &field.ty).into_token_stream(),
    );
    quote!(#(#cfgs)* #name: #value)
}

/// The value a field default `= expr` stands for. Shared with `#[data]`, whose `new` and builder
/// must initialise the field exactly as `Default` does.
pub(super) fn default_value(expr: &Expr, ty: &Type) -> Expr {
    if is_str_literal(expr) && written_as_string(ty) {
        parse_quote_spanned!(expr.span()=> ::core::convert::Into::into(#expr))
    } else {
        expr.clone()
    }
}

fn is_cfg(attr: &Attribute) -> bool {
    attr.path().is_ident("cfg")
}

fn is_str_literal(expr: &Expr) -> bool {
    matches!(
        expr,
        Expr::Lit(ExprLit {
            lit: Lit::Str(_),
            ..
        })
    )
}

fn written_as_string(ty: &Type) -> bool {
    matches!(ty, Type::Path(path) if path.qself.is_none()
        && path.path.segments.last().is_some_and(|last| last.ident == "String"
            && last.arguments.is_none()))
}

/// The span of `Default` in the struct's `#[derive(..)]`, if it derives it.
fn derived_default(attrs: &[Attribute]) -> Option<Span> {
    attrs
        .iter()
        .filter(|attr| attr.path().is_ident("derive"))
        .filter_map(|attr| {
            attr.parse_args_with(Punctuated::<Path, Token![,]>::parse_terminated)
                .ok()
        })
        .flatten()
        .find(|path| {
            path.segments
                .last()
                .is_some_and(|last| last.ident == "Default")
        })
        .map(|path| path.span())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn desugar(sugar: TokenStream) -> syn::Result<String> {
        let mut file: syn::File = syn::parse2(sugar)?;
        lower(&mut file)?;
        Ok(file.into_token_stream().to_string())
    }

    fn reprint(plain: TokenStream) -> Option<String> {
        syn::parse2::<syn::File>(plain)
            .map(|file| file.into_token_stream().to_string())
            .ok()
    }

    fn errors(sugar: TokenStream) -> Vec<String> {
        desugar(sugar)
            .err()
            .into_iter()
            .flatten()
            .map(|err| err.to_string())
            .collect()
    }

    #[test]
    fn field_defaults_become_an_impl_default_after_the_struct() {
        assert_eq!(
            desugar(quote! {
                #[derive(Debug)]
                pub struct Config { pub host: String = "127.0.0.1", pub port: u16 = 8080, tls: bool }
                fn f() {}
            })
            .ok(),
            reprint(quote! {
                #[derive(Debug)]
                pub struct Config { pub host: String, pub port: u16, tls: bool }
                impl ::core::default::Default for Config {
                    fn default() -> Self {
                        Self {
                            host: ::core::convert::Into::into("127.0.0.1"),
                            port: 8080,
                            tls: ::core::default::Default::default(),
                        }
                    }
                }
                fn f() {}
            })
        );
    }

    #[test]
    fn generics_where_clause_and_cfg_attributes_are_copied() {
        assert_eq!(
            desugar(quote! {
                #[cfg(unix)]
                struct S<T: Clone = u8> where T: Copy { #[cfg(test)] t: T = T::MIN, n: u8 }
            })
            .ok(),
            reprint(quote! {
                #[cfg(unix)]
                struct S<T: Clone = u8> where T: Copy { #[cfg(test)] t: T, n: u8 }
                #[cfg(unix)]
                impl<T: Clone> ::core::default::Default for S<T> where T: Copy {
                    fn default() -> Self {
                        Self { #[cfg(test)] t: T::MIN, n: ::core::default::Default::default(), }
                    }
                }
            })
        );
    }

    #[test]
    fn only_a_string_literal_on_a_field_written_as_string_is_converted() {
        let out = desugar(quote! {
            struct S { a: std::string::String = "a", b: &'static str = "b", c: String = s() }
        })
        .unwrap_or_default();
        assert!(
            out.contains(r#"a : :: core :: convert :: Into :: into ("a")"#),
            "{out}"
        );
        assert!(
            out.contains(r#"b : "b""#) && out.contains("c : s ()"),
            "{out}"
        );
    }

    #[test]
    fn structs_in_modules_and_blocks_get_their_impl_next_to_them() {
        assert_eq!(
            desugar(quote! { mod m { struct A { a: u8 = 1 } } fn f() { struct B { b: u8 = 2 } } })
                .ok(),
            reprint(quote! {
                mod m {
                    struct A { a: u8 }
                    impl ::core::default::Default for A { fn default() -> Self { Self { a: 1, } } }
                }
                fn f() {
                    struct B { b: u8 }
                    impl ::core::default::Default for B { fn default() -> Self { Self { b: 2, } } }
                }
            })
        );
    }

    #[test]
    fn a_bare_rest_in_a_struct_expression_becomes_default() {
        assert_eq!(
            desugar(quote! { fn f() -> S { let s = S { a: 1, .. }; T { .. }; s } }).ok(),
            reprint(quote! {
                fn f() -> S {
                    let s = S { a: 1, ..::core::default::Default::default() };
                    T { ..::core::default::Default::default() };
                    s
                }
            })
        );
    }

    #[test]
    fn struct_patterns_keep_their_rest() {
        let valid = quote! {
            fn f(S { a, .. }: S, s: S) {
                let S { b, .. } = s;
                if let S { c: 1, .. } = s {}
                match s { S { d, .. } | T { d, .. } => {} }
                let g = |S { e, .. }: S| e;
                S { a, .. } = s;
                (S { a, .. }, b) = (s, 1);
                assert!(matches!(s, S { a: 1, .. }));
                let t = S { a, ..s };
            }
        };
        assert_eq!(desugar(valid.clone()).ok(), reprint(valid));
    }

    #[test]
    fn std_expression_macros_get_their_rest_filled() {
        assert_eq!(
            desugar(
                quote! { fn f() { let v = vec![S { .. }]; assert_eq!(v[0], S { a: 1, .. }); } }
            )
            .ok(),
            reprint(quote! {
                fn f() {
                    let v = vec![S { ..::core::default::Default::default() }];
                    assert_eq!(v[0], S { a: 1, ..::core::default::Default::default() });
                }
            })
        );
    }

    #[test]
    fn a_repeated_vec_element_gets_its_rest_filled() {
        assert_eq!(
            desugar(quote! { fn f() { let v = vec![S { .. }; 2]; } }).ok(),
            reprint(quote! {
                fn f() { let v = vec![S { ..::core::default::Default::default() }; 2]; }
            })
        );
    }

    #[test]
    fn valid_rust_passes_through_unchanged() {
        let valid = quote! {
            #[derive(Default)]
            struct S<T> { a: u8, t: T }
            struct P(u8);
            enum E { A { x: u8 }, B(u8) }
            fn f() { let s = S { a: 1, ..S::default() }; let r = ..; let a = &[1, 2][..]; }
        };
        assert_eq!(desugar(valid.clone()).ok(), reprint(valid));
    }

    #[test]
    fn deriving_default_next_to_field_defaults_is_an_error() {
        assert_eq!(
            errors(quote! { #[derive(Clone, core::default::Default)] struct S { a: u8 = 1 } }),
            [
                "`Default` cannot be derived for a struct with field defaults; remove it from \
                 `#[derive(..)]`, the field defaults generate `impl Default`"
            ]
        );
    }

    #[test]
    fn defaults_on_enum_variants_and_unions_are_errors_reported_together() {
        assert_eq!(
            errors(quote! {
                enum E { A { x: u8 = 1, y: u8 = 2 } }
                union U { a: u32 = 0 }
            }),
            [
                "enum variant fields take no defaults; only struct fields do",
                "enum variant fields take no defaults; only struct fields do",
                "union fields take no defaults; only struct fields do",
            ]
        );
    }

    #[test]
    fn a_default_on_a_tuple_struct_is_rejected_while_parsing() {
        assert_eq!(
            errors(quote! { struct P(u8 = 1); }),
            ["field default value is only supported in structs with named fields"]
        );
    }
}
