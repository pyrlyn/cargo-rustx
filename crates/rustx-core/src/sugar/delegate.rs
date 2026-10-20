//! Forwarding impls back to `#[delegate(Trait)]` on the field: the reverse of
//! `passes/delegate.rs` (problem 07, trait delegation; T19.19).
//!
//! A struct followed directly by an impl the pass generates for it,
//!
//! ```text
//! struct Logged { inner: Vec<u8>, lines: usize }
//! impl ::std::io::Write for Logged {
//!     #[inline]
//!     fn write(&mut self, __rustx_arg0: &[u8]) -> ::std::io::Result<usize> {
//!         ::std::io::Write::write(&mut self.inner, __rustx_arg0)
//!     }
//!     #[inline]
//!     fn flush(&mut self) -> ::std::io::Result<()> { ::std::io::Write::flush(&mut self.inner) }
//! }
//! ```
//!
//! becomes the struct with `#[delegate(Write)]` on `inner` and no impl. The field is the one the
//! impl's first method calls the trait on. The attribute names the trait by its last segment, or
//! as the impl writes it when that name means another trait to the pass (one the file declares).
//! Nothing else is matched here: the rule expands the struct it would write, next to the file's
//! traits since the pass reads their methods, and keeps it only when that gives back the struct
//! and the impls. So an impl that forwards only some methods, adapts an argument or a result,
//! carries a doc comment or an attribute, has generics or bounds other than the struct's, or
//! implements a trait the pass cannot list (neither in its std table nor declared in the file, or
//! declared with generic parameters) is left alone.
//!
//! Several impls after one struct become one attribute per field, as long as they come in the
//! order the pass writes them, by field and then by trait; the run ends at the first impl that
//! does not read back or is out of that order, and the impls from there on stay.
//!
//! Refused as well: an impl that is not right after its struct or the impls before it, and
//! anything that would drop a comment between the struct and the last impl removed.

use proc_macro2::TokenStream;
use quote::{ToTokens, quote};
use syn::visit::{self, Visit};
use syn::{Expr, Field, ImplItem, Item, ItemImpl, ItemStruct, ItemTrait, Member, Stmt};

use super::item_lists::for_each_list;
use super::std_paths::StdPaths;
use super::{Edit, ReverseRule, apply, has_comment, span_range};

pub(crate) struct Delegate;

impl ReverseRule for Delegate {
    fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit> {
        self.edit_groups(file, source).concat()
    }

    /// One set per struct: its attributes and the removal of its impls, since either half alone
    /// changes the tokens.
    fn edit_groups(&self, file: &syn::File, source: &str) -> Vec<Vec<Edit>> {
        let mut traits = Traits::default();
        traits.visit_file(file);
        let paths = StdPaths::of(file);
        let mut finder = Finder {
            source,
            paths: &paths,
            traits: traits.0,
            groups: Vec::new(),
        };
        for_each_list(file, |items| finder.list(items));
        finder.groups
    }
}

/// Every trait the file declares, at any depth, as the pass collects them.
#[derive(Default)]
struct Traits(TokenStream);

impl<'ast> Visit<'ast> for Traits {
    fn visit_item_trait(&mut self, item: &'ast ItemTrait) {
        item.to_tokens(&mut self.0);
        visit::visit_item_trait(self, item);
    }
}

struct Finder<'a> {
    source: &'a str,
    paths: &'a StdPaths,
    traits: TokenStream,
    groups: Vec<Vec<Edit>>,
}

/// An impl that reads back as one delegation on its own.
struct Forward<'i> {
    imp: &'i ItemImpl,
    /// Index of the field it forwards to.
    field: usize,
    /// The trait as the attribute names it.
    name: String,
}

impl Finder<'_> {
    fn list(&mut self, items: &[Option<&Item>]) {
        for (at, item) in items.iter().enumerate() {
            let Some(Item::Struct(owner)) = item else {
                continue;
            };
            let mut run: Vec<Forward> = items[at + 1..]
                .iter()
                .map_while(|next| match next {
                    Some(Item::Impl(imp)) => self.forward(owner, imp),
                    _ => None,
                })
                .collect();
            // Each impl reads back alone, so a run that does not as a whole has one out of the
            // pass's order; the impls before it still convert.
            while !run.is_empty() {
                if let Some(group) = self.group(owner, &run) {
                    self.groups.push(group);
                    break;
                }
                run.pop();
            }
        }
    }

    /// `imp` as a delegation of `owner`, if the struct with that one attribute expands to the
    /// struct and `imp`.
    fn forward<'i>(&self, owner: &ItemStruct, imp: &'i ItemImpl) -> Option<Forward<'i>> {
        let (path, _) = imp.trait_.as_ref()?;
        let field = forwarded_field(owner, imp)?;
        let written = self.source.get(span_range(path.to_token_stream())?)?;
        let last = path.segments.last()?;
        let short = last
            .arguments
            .is_none()
            .then(|| last.ident.to_string())
            .filter(|short| short != written);
        short
            .into_iter()
            .chain([written.to_owned()])
            .map(|name| Forward { imp, field, name })
            .find(|forward| {
                self.attributes(owner, std::slice::from_ref(forward))
                    .and_then(|inserts| self.sugar_struct(owner, &inserts))
                    .is_some_and(|sugar| self.reads_back(&sugar, owner, &[imp]))
            })
    }

    /// The attributes of `run` and the removal of its impls, if together they read back and drop
    /// no comment.
    fn group(&self, owner: &ItemStruct, run: &[Forward]) -> Option<Vec<Edit>> {
        let end = span_range(owner.to_token_stream())?.end;
        let last = span_range(run.last()?.imp.to_token_stream())?.end;
        if has_comment(self.source.get(end..last)?) {
            return None;
        }
        let mut edits = self.attributes(owner, run)?;
        let imps: Vec<&ItemImpl> = run.iter().map(|forward| forward.imp).collect();
        if !self.reads_back(&self.sugar_struct(owner, &edits)?, owner, &imps) {
            return None;
        }
        edits.push(Edit {
            range: end..last,
            text: String::new(),
        });
        Some(edits)
    }

    /// One `#[delegate(..)]` insertion per field of `run`, after the field's own attributes: on a
    /// line of its own when the field starts one, so rustfmt's layout survives.
    fn attributes(&self, owner: &ItemStruct, run: &[Forward]) -> Option<Vec<Edit>> {
        let mut edits = Vec::new();
        for (index, field) in owner.fields.iter().enumerate() {
            let names: Vec<&str> = run
                .iter()
                .filter(|forward| forward.field == index)
                .map(|forward| forward.name.as_str())
                .collect();
            if names.is_empty() {
                continue;
            }
            let at = after_attributes(field)?;
            let line = self.source.get(..at)?.rsplit('\n').next()?;
            let gap = if line.trim().is_empty() {
                format!("\n{line}")
            } else {
                " ".to_owned()
            };
            edits.push(Edit {
                range: at..at,
                text: format!("#[delegate({})]{gap}", names.join(", ")),
            });
        }
        Some(edits)
    }

    /// The source of `owner` with `inserts`, given in source offsets, applied.
    fn sugar_struct(&self, owner: &ItemStruct, inserts: &[Edit]) -> Option<String> {
        let range = span_range(owner.to_token_stream())?;
        let inserts: Vec<Edit> = inserts
            .iter()
            .map(|edit| Edit {
                range: edit.range.start - range.start..edit.range.end - range.start,
                text: edit.text.clone(),
            })
            .collect();
        Some(apply(self.source.get(range)?, &inserts))
    }

    /// Whether `sugar`, a struct with delegations, expands to exactly `owner` and `imps`.
    fn reads_back(&self, sugar: &str, owner: &ItemStruct, imps: &[&ItemImpl]) -> bool {
        let traits = &self.traits;
        sugar
            .parse::<TokenStream>()
            .ok()
            .and_then(|sugar| crate::try_expand(quote!(#traits #sugar)).ok())
            .is_some_and(|file| {
                self.paths.normalise(file.into_token_stream())
                    == self.paths.normalise(quote!(#traits #owner #(#imps)*))
            })
    }
}

/// The field the impl's first method calls the trait on, as in `Trait::m(&mut self.field, ..)`.
fn forwarded_field(owner: &ItemStruct, imp: &ItemImpl) -> Option<usize> {
    let method = imp.items.iter().find_map(|item| match item {
        ImplItem::Fn(method) => Some(method),
        _ => None,
    })?;
    let [Stmt::Expr(body, None)] = method.block.stmts.as_slice() else {
        return None;
    };
    let body = match body {
        Expr::Await(awaited) => &*awaited.base,
        body => body,
    };
    let Expr::Call(call) = body else {
        return None;
    };
    let receiver = match call.args.first()? {
        Expr::Reference(borrow) => &*borrow.expr,
        receiver => receiver,
    };
    let Expr::Field(access) = receiver else {
        return None;
    };
    let Expr::Path(base) = &*access.base else {
        return None;
    };
    if !base.path.is_ident("self") {
        return None;
    }
    owner
        .fields
        .iter()
        .enumerate()
        .find(|(index, field)| match (&access.member, &field.ident) {
            (Member::Named(name), Some(ident)) => name == ident,
            (Member::Unnamed(at), None) => usize::try_from(at.index).ok() == Some(*index),
            _ => false,
        })
        .map(|(index, _)| index)
}

/// Where the field starts once its own attributes and doc comments are passed.
fn after_attributes(field: &Field) -> Option<usize> {
    let mut bare = field.clone();
    bare.attrs.clear();
    Some(span_range(bare.to_token_stream())?.start)
}

#[cfg(test)]
mod tests {
    use super::Delegate;
    use crate::sugar::ReverseRule;
    use crate::sugar::sugar;
    use crate::sugar::tests::assert_sugar;

    /// The pass's output for `Logged` (its guide page), as rustfmt lays it out.
    const LOGGED: &str = "use std::io::Write;\n\npub struct Logged {\n    inner: Vec<u8>,\n    \
         lines: usize,\n}\n\nimpl ::std::io::Write for Logged {\n    #[inline]\n    \
         fn write(&mut self, __rustx_arg0: &[u8]) -> ::std::io::Result<usize> {\n        \
         ::std::io::Write::write(&mut self.inner, __rustx_arg0)\n    }\n    #[inline]\n    \
         fn flush(&mut self) -> ::std::io::Result<()> {\n        \
         ::std::io::Write::flush(&mut self.inner)\n    }\n}\n";

    const LOGGED_SUGAR: &str = "use std::io::Write;\n\npub struct Logged {\n    \
         #[delegate(Write)]\n    inner: Vec<u8>,\n    lines: usize,\n}\n";

    fn unchanged(source: &str) {
        assert_eq!(sugar(source).unwrap(), source);
    }

    #[test]
    fn a_forwarding_impl_of_a_std_table_trait_becomes_the_attribute() {
        assert_sugar(LOGGED, LOGGED_SUGAR);
        // Short std names the file imports count as the full paths the pass writes.
        let short = LOGGED
            .replace("use std::io::Write;", "use std::io::{self, Write};")
            .replace("::std::io::Write", "Write")
            .replace("::std::io::Result", "io::Result");
        assert_sugar(
            &short,
            &LOGGED_SUGAR.replace("use std::io::Write;", "use std::io::{self, Write};"),
        );
        // The impl of a generic struct names the trait right after its generics.
        let generic = |text: &str| {
            text.replace("Logged {", "Logged<W> {")
                .replace("inner: Vec<u8>", "inner: W")
                .replace("impl Write", "impl<W> Write")
        };
        assert_sugar(
            &generic(&short),
            &generic(&LOGGED_SUGAR.replace("use std::io::Write;", "use std::io::{self, Write};")),
        );
    }

    #[test]
    fn generics_where_clauses_and_tuple_fields_convert() {
        let fmt = |tr: &str| {
            format!(
                "impl<'a, T: Clone> ::core::fmt::{tr} for Show<'a, T> where T: Copy {{\n    \
                 #[inline]\n    fn fmt(&self, __rustx_arg0: &mut ::core::fmt::Formatter<'_>) -> \
                 ::core::fmt::Result {{\n        ::core::fmt::{tr}::fmt(&self.1, __rustx_arg0)\n    \
                 }}\n}}\n"
            )
        };
        assert_sugar(
            &format!(
                "struct Show<'a, T: Clone = u8>(u8, &'a T) where T: Copy;\n{}{}",
                fmt("Display"),
                fmt("Debug")
            ),
            "struct Show<'a, T: Clone = u8>(u8, #[delegate(Display, Debug)] &'a T) where T: Copy;\n",
        );
    }

    #[test]
    fn a_trait_declared_in_the_file_is_delegated_by_its_name() {
        let shape = "trait Shape {\n    const SIDES: u8;\n    fn area(&self) -> f64;\n    \
                     fn into_name(self) -> String;\n    fn sides(&self) -> u8 { Self::SIDES }\n}\n";
        assert_sugar(
            &format!(
                "{shape}mod inner {{\n    struct Tile {{ shape: Square }}\n    \
                 impl Shape for Tile {{\n        const SIDES: u8 = <Square as Shape>::SIDES;\n        \
                 #[inline]\n        fn area(&self) -> f64 {{ Shape::area(&self.shape) }}\n        \
                 #[inline]\n        fn into_name(self) -> String {{ Shape::into_name(self.shape) }}\n    \
                 }}\n}}\n"
            ),
            &format!(
                "{shape}mod inner {{\n    struct Tile {{ #[delegate(Shape)] shape: Square }}\n}}\n"
            ),
        );
    }

    #[test]
    fn several_traits_on_one_field_and_on_different_fields_convert() {
        assert_sugar(
            "fn f() -> u8 {\n    struct Tee {\n        r: File,\n        /// The sink.\n        \
             w: File,\n    }\n    \
             impl ::std::io::Read for Tee {\n        #[inline]\n        \
             fn read(&mut self, __rustx_arg0: &mut [u8]) -> ::std::io::Result<usize> {\n            \
             ::std::io::Read::read(&mut self.r, __rustx_arg0)\n        }\n    }\n    \
             impl ::std::io::Write for Tee {\n        #[inline]\n        \
             fn write(&mut self, __rustx_arg0: &[u8]) -> ::std::io::Result<usize> {\n            \
             ::std::io::Write::write(&mut self.w, __rustx_arg0)\n        }\n        #[inline]\n        \
             fn flush(&mut self) -> ::std::io::Result<()> {\n            \
             ::std::io::Write::flush(&mut self.w)\n        }\n    }\n    \
             impl ::core::fmt::Debug for Tee {\n        #[inline]\n        \
             fn fmt(&self, __rustx_arg0: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {\n            \
             ::core::fmt::Debug::fmt(&self.w, __rustx_arg0)\n        }\n    }\n    1\n}\n",
            "fn f() -> u8 {\n    struct Tee {\n        #[delegate(Read)]\n        r: File,\n        \
             /// The sink.\n        #[delegate(Write, Debug)]\n        w: File,\n    }\n    1\n}\n",
        );
    }

    /// `S` with two fields and a `fmt` impl of `tr` forwarding to `field`.
    fn fmt_impl(tr: &str, field: &str) -> String {
        format!(
            "impl ::core::fmt::{tr} for S {{ #[inline] fn fmt(&self, __rustx_arg0: &mut \
             ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {{ \
             ::core::fmt::{tr}::fmt(&self.{field}, __rustx_arg0) }} }}\n"
        )
    }

    #[test]
    fn impls_out_of_the_pass_order_convert_up_to_the_first_out_of_place() {
        let source = format!(
            "struct S {{ a: u8, b: u8 }}\n{}{}",
            fmt_impl("Display", "b"),
            fmt_impl("Debug", "a")
        );
        assert_sugar(
            &source,
            &format!(
                "struct S {{ a: u8, #[delegate(Display)] b: u8 }}\n{}",
                fmt_impl("Debug", "a")
            ),
        );
    }

    #[test]
    fn a_std_name_the_file_declares_as_a_trait_keeps_the_impl_spelling() {
        assert_sugar(
            &format!(
                "trait Display {{ fn show(&self); }}\nstruct S {{ a: u8, b: u8 }}\n{}",
                fmt_impl("Display", "a")
            ),
            "trait Display { fn show(&self); }\n\
             struct S { #[delegate(::core::fmt::Display)] a: u8, b: u8 }\n",
        );
    }

    #[test]
    fn an_impl_the_pass_would_not_generate_is_left_alone() {
        // Only some methods forwarded.
        unchanged(&LOGGED.replace(
            "    #[inline]\n    fn flush(&mut self) -> ::std::io::Result<()> {\n        \
             ::std::io::Write::flush(&mut self.inner)\n    }\n",
            "",
        ));
        // An argument or a result adapted.
        unchanged(&LOGGED.replace("__rustx_arg0)", "&__rustx_arg0[1..])"));
        unchanged(&LOGGED.replace(
            "::std::io::Write::flush(&mut self.inner)",
            "::std::io::Write::flush(&mut self.inner).map(|()| ())",
        ));
        // A method called on the field rather than through the trait, or without `#[inline]`.
        unchanged(&LOGGED.replace(
            "::std::io::Write::flush(&mut self.inner)",
            "self.inner.flush()",
        ));
        unchanged(&LOGGED.replace("    #[inline]\n", ""));
        // A doc comment or an attribute on the impl.
        unchanged(&LOGGED.replace("impl ::std", "/// Forwards.\nimpl ::std"));
        unchanged(&LOGGED.replace("impl ::std", "#[allow(unused)]\nimpl ::std"));
    }

    #[test]
    fn generics_or_bounds_other_than_the_structs_are_left_alone() {
        let read = |header: &str| {
            format!(
                "struct W<R> {{ r: R }}\n{header} {{ #[inline] fn read(&mut self, \
                 __rustx_arg0: &mut [u8]) -> ::std::io::Result<usize> {{ \
                 ::std::io::Read::read(&mut self.r, __rustx_arg0) }} }}\n"
            )
        };
        assert_sugar(
            &read("impl<R> ::std::io::Read for W<R>"),
            "struct W<R> { #[delegate(Read)] r: R }\n",
        );
        unchanged(&read("impl<R: ::std::io::Read> ::std::io::Read for W<R>"));
        unchanged(&read(
            "impl<R> ::std::io::Read for W<R> where R: ::std::io::Read",
        ));
    }

    #[test]
    fn a_trait_the_pass_cannot_list_is_left_alone() {
        // Neither in the std table nor declared in the file.
        unchanged(
            "struct Name { s: String }\nimpl AsRef<str> for Name { #[inline] fn as_ref(&self) -> \
             &str { AsRef::<str>::as_ref(&self.s) } }\n",
        );
        unchanged(
            "struct T { s: S }\nimpl other::Shape for T { #[inline] fn area(&self) -> f64 { \
             other::Shape::area(&self.s) } }\n",
        );
        // Declared with generic parameters, or twice.
        unchanged(
            "trait Conv<T> { fn conv(&self) -> T; }\nstruct S { a: A }\nimpl Conv<u8> for S { \
             #[inline] fn conv(&self) -> u8 { Conv::<u8>::conv(&self.a) } }\n",
        );
        unchanged(
            "mod a { pub trait Area { fn area(&self) -> f64; } }\n\
             mod b { pub trait Area { fn area(&self) -> f64; } }\nuse a::Area;\n\
             struct S { a: A }\nimpl Area for S { #[inline] fn area(&self) -> f64 { \
             Area::area(&self.a) } }\n",
        );
    }

    #[test]
    fn an_impl_apart_from_its_struct_or_a_comment_it_would_drop_is_left_alone() {
        unchanged(&LOGGED.replace("}\n\nimpl", "}\n\nfn between() {}\n\nimpl"));
        unchanged(&LOGGED.replace("}\n\nimpl", "}\n\n// Why.\nimpl"));
        unchanged(&LOGGED.replace("{\n    #[inline]", "{\n    // Why.\n    #[inline]"));
    }

    #[test]
    fn the_attributes_and_the_impl_removal_are_one_set() {
        let file = syn::parse_file(LOGGED).unwrap();
        let groups = Delegate.edit_groups(&file, LOGGED);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
    }
}
