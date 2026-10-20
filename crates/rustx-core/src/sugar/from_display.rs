//! `From`, `Display` and `Error` impls back to `from` variants and `#[display(..)]`: the reverse
//! of `passes/from_display.rs` (problem 05, T19.17).
//!
//! An enum followed directly by the impls the pass generates for it,
//!
//! ```text
//! enum E { Io(io::Error), Missing { key: String } }
//! impl ::core::convert::From<io::Error> for E {
//!     fn from(__rustx_value: io::Error) -> Self { Self::Io(__rustx_value) }
//! }
//! impl ::core::fmt::Display for E {
//!     fn fmt(&self, __rustx_f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
//!         match self {
//!             Self::Io(__rustx_arg0) => ::core::write!(__rustx_f, "io: {0}", __rustx_arg0),
//!             Self::Missing { key: __rustx_arg0, .. } => ::core::write!(__rustx_f, "no {0}", __rustx_arg0),
//!         }
//!     }
//! }
//! impl ::std::error::Error for E {}
//! ```
//!
//! becomes `enum E { #[display("io: {0}")] Io(from io::Error), #[display("no {key}")] Missing
//! { key: String } }`. The rule only reads which variant each `From` impl builds, and each
//! `write!`'s string with the fields its arguments bind; it writes the enum that would give them
//! and keeps it only when that enum expands to exactly itself and those impls. So generics, the
//! `where` clause, other bounds, the derives (a derived `Display` keeps its attributes, a derived
//! `Error` gets no impl) and the body of every impl are checked against the pass itself.
//!
//! The longest run of impls that reads back wins: the `From` impls with `Display` and `Error`,
//! then with `Display` alone, then fewer and fewer `From` impls. The impls after the run stay as
//! written, such as a `Display` the attribute cannot express or an `Error` with `source()`.
//!
//! The pass binds its own names (`__rustx_value`, `__rustx_f`, `__rustx_arg0`) and writes each
//! arm without a block; the round trip compares tokens, so a hand-written `fn from(e: ..)` or an
//! arm in `{ .. }` stays. Refused as well: an enum with a variant two `From` impls build, and
//! removed impls holding a comment, or with a comment between them.

use proc_macro2::{Literal, TokenStream};
use quote::{ToTokens, quote};
use syn::ext::IdentExt;
use syn::parse::Parser;
use syn::punctuated::Punctuated;
use syn::{
    Arm, Expr, Fields, ImplItem, Item, ItemEnum, ItemImpl, Lit, Pat, Path, Stmt, Token, Type,
    Variant,
};

use super::item_lists::for_each_list;
use super::std_paths::StdPaths;
use super::{Edit, ReverseRule, apply, has_comment, span_range};

pub(crate) struct FromDisplay;

impl ReverseRule for FromDisplay {
    fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit> {
        self.edit_groups(file, source).concat()
    }

    /// One set per enum: the `from`s and attributes go in only with its impls gone, and either
    /// half alone changes the tokens.
    fn edit_groups(&self, file: &syn::File, source: &str) -> Vec<Vec<Edit>> {
        let paths = StdPaths::of(file);
        let mut groups = Vec::new();
        for_each_list(file, |items| {
            for (at, item) in items.iter().enumerate() {
                let Some(Item::Enum(item)) = item else {
                    continue;
                };
                let impls: Vec<&ItemImpl> = items[at + 1..]
                    .iter()
                    .map_while(|next| match next {
                        Some(Item::Impl(imp)) => Some(imp),
                        _ => None,
                    })
                    .collect();
                let unit = Unit {
                    source,
                    paths: &paths,
                    item,
                };
                groups.extend(unit.edits(&impls));
            }
        });
        groups
    }
}

/// An enum and what it takes to write it as sugar.
struct Unit<'a> {
    source: &'a str,
    paths: &'a StdPaths,
    item: &'a ItemEnum,
}

/// What a `Display` impl says per variant: the `#[display]` string.
struct Display {
    strings: Vec<String>,
}

impl Unit<'_> {
    fn edits(&self, impls: &[&ItemImpl]) -> Option<Vec<Edit>> {
        let froms: Vec<usize> = impls.iter().map_while(|imp| self.built(imp)).collect();
        let repeated = froms
            .iter()
            .enumerate()
            .any(|(k, variant)| froms[..k].contains(variant));
        if repeated {
            return None;
        }
        let display = impls.get(froms.len()).and_then(|imp| self.display(imp));
        let mut tries: Vec<(usize, Option<&Display>, usize)> = Vec::new();
        if let Some(display) = &display {
            if impls.get(froms.len() + 1).is_some_and(|imp| is_error(imp)) {
                tries.push((froms.len(), Some(display), froms.len() + 2));
            }
            tries.push((froms.len(), Some(display), froms.len() + 1));
        }
        tries.extend((1..=froms.len()).rev().map(|n| (n, None, n)));
        tries
            .into_iter()
            .find_map(|(n, display, taken)| self.convert(&froms[..n], display, &impls[..taken]))
    }

    /// The edits writing `froms` and `display` into the enum and removing `impls`, when the
    /// result expands back to the enum and exactly those impls.
    fn convert(
        &self,
        froms: &[usize],
        display: Option<&Display>,
        impls: &[&ItemImpl],
    ) -> Option<Vec<Edit>> {
        let variants: Vec<&Variant> = self.item.variants.iter().collect();
        let mut edits = Vec::new();
        for &index in froms {
            let Fields::Unnamed(fields) = &variants[index].fields else {
                return None;
            };
            // Rewrites the character before the type rather than inserting at its start, which
            // is where another rule's edit of the type starts: the driver then sees two edits
            // that do not overlap and keeps whichever expands back, instead of dropping this set.
            let at = span_range(fields.unnamed.first()?.ty.to_token_stream())?.start;
            let before = self.source.get(..at)?.chars().next_back()?;
            edits.push(Edit {
                range: at - before.len_utf8()..at,
                text: format!("{before}from "),
            });
        }
        if let Some(display) = display {
            for (variant, string) in variants.iter().zip(&display.strings) {
                let at = span_range(variant.to_token_stream())?.start;
                let attr = format!("#[display({})]", Literal::string(string));
                edits.push(Edit {
                    range: at..at,
                    text: format!("{attr}{}", self.separator(at)?),
                });
            }
        }
        edits.sort_by_key(|edit| edit.range.start);

        let whole = span_range(self.item.to_token_stream())?;
        let shifted: Vec<Edit> = edits
            .iter()
            .map(|edit| Edit {
                range: edit.range.start - whole.start..edit.range.end - whole.start,
                text: edit.text.clone(),
            })
            .collect();
        let sugar = apply(self.source.get(whole.clone())?, &shifted);
        if !self.reads_back(&sugar, impls) {
            return None;
        }

        let last = span_range(impls.last()?.to_token_stream())?;
        if has_comment(self.source.get(whole.end..last.end)?) {
            return None;
        }
        edits.push(Edit {
            range: whole.end..last.end,
            text: String::new(),
        });
        Some(edits)
    }

    /// The index of the one-field tuple variant `imp` builds, when it is a `From` impl for this
    /// enum whose only item is a `from` building `Self::V(..)` and nothing else.
    fn built(&self, imp: &ItemImpl) -> Option<usize> {
        if !self.for_this_enum(imp, "From") {
            return None;
        }
        let [ImplItem::Fn(func)] = imp.items.as_slice() else {
            return None;
        };
        let [Stmt::Expr(Expr::Call(call), None)] = func.block.stmts.as_slice() else {
            return None;
        };
        let Expr::Path(path) = &*call.func else {
            return None;
        };
        let name = &path.path.segments.last()?.ident;
        let index = self.item.variants.iter().position(|v| v.ident == *name)?;
        match &self.item.variants[index].fields {
            Fields::Unnamed(fields) if fields.unnamed.len() == 1 && call.args.len() == 1 => {
                Some(index)
            }
            _ => None,
        }
    }

    /// The `#[display]` strings a `Display` impl for this enum would come from: one `match self`
    /// with one `write!` arm per variant, in order.
    fn display(&self, imp: &ItemImpl) -> Option<Display> {
        if !self.for_this_enum(imp, "Display") {
            return None;
        }
        let [ImplItem::Fn(func)] = imp.items.as_slice() else {
            return None;
        };
        let [Stmt::Expr(Expr::Match(matched), None)] = func.block.stmts.as_slice() else {
            return None;
        };
        if matched.arms.len() != self.item.variants.len() {
            return None;
        }
        let strings = matched
            .arms
            .iter()
            .zip(&self.item.variants)
            .map(|(arm, variant)| display_string(arm, variant))
            .collect::<Option<Vec<String>>>()?;
        Some(Display { strings })
    }

    /// Whether `imp` implements a trait whose last segment is `name` for this enum. The rest of
    /// the path, modifiers such as `!`, the generics and the bounds are left to the expansion
    /// check.
    fn for_this_enum(&self, imp: &ItemImpl, name: &str) -> bool {
        imp.trait_
            .as_ref()
            .is_some_and(|(path, _)| last_is(path, name))
            && matches!(&*imp.self_ty, Type::Path(ty) if last_is(&ty.path, &self.item.ident.to_string()))
    }

    /// What goes after an attribute put before the item at `at`: a line break and the same
    /// indentation when the item starts its line, else a space.
    fn separator(&self, at: usize) -> Option<String> {
        let before = self.source.get(..at)?;
        let line = before.rfind('\n').map_or(before, |i| &before[i + 1..]);
        Some(if before.contains('\n') && line.trim().is_empty() {
            format!("\n{line}")
        } else {
            " ".to_owned()
        })
    }

    /// Whether `sugar`, the enum with its `from`s and attributes, expands to exactly the enum
    /// and `impls`.
    fn reads_back(&self, sugar: &str, impls: &[&ItemImpl]) -> bool {
        let item = self.item;
        sugar
            .parse::<TokenStream>()
            .ok()
            .and_then(|tokens| crate::try_expand(tokens).ok())
            .is_some_and(|file| {
                self.paths.normalise(file.into_token_stream())
                    == self.paths.normalise(quote!(#item #(#impls)*))
            })
    }
}

/// An `Error` impl with no items, the only one the pass writes.
fn is_error(imp: &ItemImpl) -> bool {
    imp.items.is_empty()
        && imp
            .trait_
            .as_ref()
            .is_some_and(|(path, _)| last_is(path, "Error"))
}

/// The `#[display]` string of `variant` from its arm `Self::V(a, _) => write!(f, "..", a),`:
/// each placeholder `{k}` names the field the `k`th
/// argument binds, `{0}` for a tuple field and `{name}` for a named one.
fn display_string(arm: &Arm, variant: &Variant) -> Option<String> {
    // Each binding of the pattern with the placeholder that names its field.
    let mut bindings: Vec<(String, String)> = Vec::new();
    let path = match (&arm.pat, &variant.fields) {
        (Pat::Path(pat), Fields::Unit) => &pat.path,
        (Pat::TupleStruct(pat), Fields::Unnamed(fields)) => {
            if pat.elems.len() != fields.unnamed.len() {
                return None;
            }
            for (index, elem) in pat.elems.iter().enumerate() {
                match elem {
                    Pat::Wild(_) => {}
                    elem => bindings.push((binding(elem)?, index.to_string())),
                }
            }
            &pat.path
        }
        (Pat::Struct(pat), Fields::Named(_)) => {
            pat.rest.as_ref()?;
            for field in &pat.fields {
                let syn::Member::Named(name) = &field.member else {
                    return None;
                };
                bindings.push((binding(&field.pat)?, name.unraw().to_string()));
            }
            &pat.path
        }
        _ => return None,
    };
    if !last_is(path, &variant.ident.to_string()) {
        return None;
    }
    let Expr::Macro(write) = &*arm.body else {
        return None;
    };
    if !last_is(&write.mac.path, "write") {
        return None;
    }
    let args = Punctuated::<Expr, Token![,]>::parse_terminated
        .parse2(write.mac.tokens.clone())
        .ok()?;
    let mut args = args.iter();
    args.next()?;
    let Some(Expr::Lit(syn::ExprLit {
        lit: Lit::Str(text),
        ..
    })) = args.next()
    else {
        return None;
    };
    let names = args
        .map(|arg| {
            let Expr::Path(arg) = arg else {
                return None;
            };
            let arg = arg.path.get_ident()?.to_string();
            let (_, name) = bindings.iter().find(|(bound, _)| *bound == arg)?;
            Some(name.as_str())
        })
        .collect::<Option<Vec<&str>>>()?;
    unformat(&text.value(), &names)
}

/// The name a pattern binds, when it is a plain binding.
fn binding(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(id) if id.by_ref.is_none() && id.mutability.is_none() && id.subpat.is_none() => {
            Some(id.ident.to_string())
        }
        _ => None,
    }
}

/// `text` with each positional placeholder `{k:spec}` written as `{name:spec}` for the `k`th of
/// `names`; `None` for any other placeholder, which the pass never writes.
fn unformat(text: &str, names: &[&str]) -> Option<String> {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(at) = rest.find(['{', '}']) {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        if tail.starts_with("{{") || tail.starts_with("}}") {
            out.push_str(&tail[..2]);
            rest = &tail[2..];
            continue;
        }
        let close = tail.find('}').filter(|_| tail.starts_with('{'))?;
        let placeholder = &tail[1..close];
        let (arg, spec) = match placeholder.split_once(':') {
            Some((arg, spec)) => (arg, Some(spec)),
            None => (placeholder, None),
        };
        if arg.is_empty() || !arg.bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        out.push('{');
        out.push_str(names.get(arg.parse::<usize>().ok()?)?);
        if let Some(spec) = spec {
            out.push(':');
            out.push_str(spec);
        }
        out.push('}');
        rest = &tail[close + 1..];
    }
    out.push_str(rest);
    Some(out)
}

fn last_is(path: &Path, name: &str) -> bool {
    path.segments.last().is_some_and(|last| last.ident == name)
}

#[cfg(test)]
mod tests {
    use super::{FromDisplay, unformat};
    use crate::sugar::tests::assert_sugar;
    use crate::sugar::{Edit, ReverseRule, convert, sugar};

    /// The pass's `From` impl for `variant` holding `ty`, of the enum `E`.
    fn from(ty: &str, variant: &str) -> String {
        format!(
            "impl ::core::convert::From<{ty}> for E {{\n    \
             fn from(__rustx_value: {ty}) -> Self {{ Self::{variant}(__rustx_value) }}\n}}\n"
        )
    }

    /// The pass's `Display` impl of `E` with these arms.
    fn display(arms: &str) -> String {
        format!(
            "impl ::core::fmt::Display for E {{\n    \
             fn fmt(&self, __rustx_f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {{\n        \
             match self {{\n{arms}        }}\n    }}\n}}\n"
        )
    }

    const ERROR: &str = "impl ::std::error::Error for E {}\n";

    /// Puts a space after the first `(`, an edit that keeps the meaning.
    struct SpaceInParen;

    impl ReverseRule for SpaceInParen {
        fn edits(&self, _: &syn::File, source: &str) -> Vec<Edit> {
            let at = source.find('(').unwrap();
            vec![Edit {
                range: at..at + 1,
                text: "( ".to_owned(),
            }]
        }
    }

    const LOAD: &str = "enum E {\n    Io(io::Error),\n    Parse(ParseIntError),\n    Missing { key: String },\n}\n";

    const LOAD_ARMS: &str = "            \
        Self::Io(__rustx_arg0) => ::core::write!(__rustx_f, \"io error: {0}\", __rustx_arg0),\n            \
        Self::Parse(__rustx_arg0) => ::core::write!(__rustx_f, \"bad number: {0:?}\", __rustx_arg0),\n            \
        Self::Missing { key: __rustx_arg0, .. } => ::core::write!(__rustx_f, \"missing {{key}} {0}\", __rustx_arg0),\n";

    const LOAD_SUGAR: &str = "enum E {\n    #[display(\"io error: {0}\")]\n    Io(from io::Error),\n    \
        #[display(\"bad number: {0:?}\")]\n    Parse(from ParseIntError),\n    \
        #[display(\"missing {{key}} {key}\")]\n    Missing { key: String },\n}\n";

    fn load() -> String {
        format!(
            "{LOAD}{}{}{}{ERROR}",
            from("io::Error", "Io"),
            from("ParseIntError", "Parse"),
            display(LOAD_ARMS)
        )
    }

    fn unchanged(source: &str) {
        assert_eq!(sugar(source).unwrap(), source);
    }

    #[test]
    fn from_display_and_error_impls_become_from_variants_and_display_attributes() {
        assert_sugar(
            &format!("{}fn f() {{}}\n", load()),
            &format!("{LOAD_SUGAR}fn f() {{}}\n"),
        );
    }

    #[test]
    fn from_impls_alone_become_from_variants_in_any_item_list() {
        let source = format!(
            "mod m {{\n{LOAD}{}{}}}\nfn g() {{\n    enum E {{ A(u8) }}\n    {}    let x = 1;\n}}\n",
            from("io::Error", "Io"),
            from("ParseIntError", "Parse"),
            from("u8", "A")
        );
        assert_sugar(
            &source,
            "mod m {\nenum E {\n    Io(from io::Error),\n    Parse(from ParseIntError),\n    \
             Missing { key: String },\n}\n}\nfn g() {\n    enum E { A(from u8) }\n    let x = 1;\n}\n",
        );
    }

    #[test]
    fn display_strings_name_the_fields_their_arguments_bind() {
        let arms = "            \
            Self::Two(_, __rustx_arg0, __rustx_arg1) => ::core::write!(__rustx_f, \"{0:>4} {1} {0}\", __rustx_arg0, __rustx_arg1),\n            \
            Self::Pair { r#b: __rustx_arg0, a: __rustx_arg1, .. } => ::core::write!(__rustx_f, \"{0} of {1:?}\", __rustx_arg0, __rustx_arg1),\n            \
            Self::None => ::core::write!(__rustx_f, \"none\"),\n";
        assert_sugar(
            &format!(
                "enum E {{ Two(u8, u16, u32), Pair {{ a: u8, r#b: u16 }}, None }}\n{}{ERROR}",
                display(arms)
            ),
            "enum E { #[display(\"{1:>4} {2} {1}\")] Two(u8, u16, u32), \
             #[display(\"{b} of {a:?}\")] Pair { a: u8, r#b: u16 }, #[display(\"none\")] None }\n",
        );
    }

    #[test]
    fn generics_and_the_where_clause_convert_as_the_pass_copies_them() {
        assert_sugar(
            "enum E<T: Copy, const N: usize = 1> where T: Default { A([T; N]), B(u8) }\n\
             impl<T: Copy, const N: usize> ::core::convert::From<[T; N]> for E<T, N> where T: Default {\n    \
             fn from(__rustx_value: [T; N]) -> Self { Self::A(__rustx_value) }\n}\n",
            "enum E<T: Copy, const N: usize = 1> where T: Default { A(from [T; N]), B(u8) }\n",
        );
    }

    #[test]
    fn an_enum_deriving_error_converts_without_an_error_impl() {
        let arms = "            Self::A => ::core::write!(__rustx_f, \"a\"),\n";
        assert_sugar(
            &format!(
                "#[derive(Debug, thiserror::Error)]\nenum E {{\n    A,\n}}\n{}",
                display(arms)
            ),
            "#[derive(Debug, thiserror::Error)]\nenum E {\n    #[display(\"a\")]\n    A,\n}\n",
        );
    }

    #[test]
    fn an_impl_the_pass_would_not_generate_is_left_alone() {
        // Another statement, another binding name, or a bound the pass does not add.
        unchanged(&format!(
            "enum E {{ A(u8) }}\n{}",
            from("u8", "A").replace("Self::A", "let _ = 1; Self::A")
        ));
        unchanged("enum E { A(u8) }\nimpl From<u8> for E { fn from(e: u8) -> Self { E::A(e) } }\n");
        unchanged(
            "enum E<T> { A(T) }\nimpl<T: Clone> ::core::convert::From<T> for E<T> {\n    \
             fn from(__rustx_value: T) -> Self { Self::A(__rustx_value) }\n}\n",
        );
        // A variant with two fields, or a `From` impl for another type.
        unchanged(&format!("enum E {{ A(u8, u8) }}\n{}", from("u8", "A")));
        unchanged(&format!("enum F {{ A(u8) }}\n{}", from("u8", "A")));
    }

    #[test]
    fn a_variant_two_from_impls_build_is_left_alone() {
        unchanged(&format!(
            "enum E {{ A(u8), B(u16) }}\n{}{}",
            from("u8", "A"),
            from("u16", "A")
        ));
    }

    #[test]
    fn a_display_the_attribute_cannot_express_stays_while_from_impls_convert() {
        let froms = from("io::Error", "Io") + &from("ParseIntError", "Parse");
        let kept = |arms: &str| {
            let source = format!("{LOAD}{froms}{}{ERROR}", display(arms));
            let expected = format!(
                "enum E {{\n    Io(from io::Error),\n    Parse(from ParseIntError),\n    \
                 Missing {{ key: String }},\n}}\n{}{ERROR}",
                display(arms)
            );
            assert_sugar(&source, &expected);
        };
        // A call in an argument, an implicit placeholder, and an arm in a block.
        kept(&LOAD_ARMS.replace(
            "\"io error: {0}\", __rustx_arg0)",
            "\"io error: {0}\", __rustx_arg0.kind())",
        ));
        kept(&LOAD_ARMS.replace("io error: {0}", "io error: {}"));
        kept(&LOAD_ARMS.replace(
            "=> ::core::write!(__rustx_f, \"io error: {0}\", __rustx_arg0),",
            "=> { ::core::write!(__rustx_f, \"io error: {0}\", __rustx_arg0) }",
        ));
        // An arm missing, so a variant has no string.
        kept(&LOAD_ARMS.replace(
            "            Self::Parse(__rustx_arg0) => ::core::write!(__rustx_f, \"bad number: {0:?}\", __rustx_arg0),\n",
            "            _ => ::core::write!(__rustx_f, \"other\"),\n",
        ));
    }

    #[test]
    fn an_error_impl_with_source_stays_with_its_display() {
        let error = "impl ::std::error::Error for E {\n    \
                     fn source(&self) -> Option<&(dyn ::std::error::Error + 'static)> { None }\n}\n";
        let arms = "            Self::A(__rustx_arg0) => ::core::write!(__rustx_f, \"a {0}\", __rustx_arg0),\n";
        assert_sugar(
            &format!(
                "enum E {{ A(u8) }}\n{}{}{error}",
                from("u8", "A"),
                display(arms)
            ),
            &format!("enum E {{ A(from u8) }}\n{}{error}", display(arms)),
        );
    }

    #[test]
    fn impls_apart_from_their_enum_or_with_a_comment_are_left_alone() {
        unchanged(&format!(
            "enum E {{ A(u8) }}\nfn f() {{}}\n{}",
            from("u8", "A")
        ));
        unchanged(&format!("enum E {{ A(u8) }}\n// why\n{}", from("u8", "A")));
        unchanged(&format!(
            "enum E {{ A(u8) }}\n{}",
            from("u8", "A").replace("Self::A", "/* why */ Self::A")
        ));
        // A comment inside the enum stays, and a `//` inside a display string is no comment.
        let arms = "            Self::A => ::core::write!(__rustx_f, \"http://a\"),\n";
        assert_sugar(
            &format!("enum E {{ /* why */ A }}\n{}{ERROR}", display(arms)),
            "enum E { /* why */ #[display(\"http://a\")] A }\n",
        );
    }

    #[test]
    fn the_enum_and_its_impls_are_one_set_kept_or_dropped_whole() {
        let source = format!("enum E {{ A(u8) }}\n{}", from("u8", "A"));
        let file = syn::parse_file(&source).unwrap();
        let groups = FromDisplay.edit_groups(&file, &source);
        assert_eq!(groups.len(), 1);
        assert_eq!(groups[0].len(), 2);
        // An earlier edit overlapping the `from` drops the removal of the impl with it.
        assert_eq!(
            convert(&source, &[&SpaceInParen, &FromDisplay]).unwrap(),
            source.replacen('(', "( ", 1)
        );
    }

    #[test]
    fn a_variant_type_another_rule_rewrites_still_takes_from_with_the_other_variants() {
        // `prefix_types` would write `~std::error::Error` there, but `from ~T` does not expand
        // back (`from` before a path is a type named `from`), so the driver keeps `from` instead.
        let boxed = "Box<dyn std::error::Error + Send + Sync>";
        assert_sugar(
            &format!(
                "enum E {{ Other({boxed}), A(u8) }}\n{}{}",
                from(boxed, "Other"),
                from("u8", "A")
            ),
            &format!("enum E {{ Other(from {boxed}), A(from u8) }}\n"),
        );
    }

    #[test]
    fn rustfmt_layout_with_trailing_commas_in_where_lists_converts() {
        let source = "pub enum E<T>\nwhere\n    T: Copy,\n{\n    A(T),\n    B(u8, u16),\n}\n\
            impl<T> ::core::convert::From<T> for E<T>\nwhere\n    T: Copy,\n{\n    \
            fn from(__rustx_value: T) -> Self {\n        Self::A(__rustx_value)\n    }\n}\n\
            impl<T> ::core::fmt::Display for E<T>\nwhere\n    T: Copy,\n{\n    \
            fn fmt(&self, __rustx_f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {\n        \
            match self {\n            Self::A(_) => ::core::write!(__rustx_f, \"a\"),\n            \
            Self::B(__rustx_arg0, __rustx_arg1) => ::core::write!(\n                __rustx_f,\n                \
            \"a long message about {0} and then {1}, long enough for rustfmt to break it\",\n                \
            __rustx_arg0,\n                __rustx_arg1\n            ),\n        }\n    }\n}\n\
            impl<T> ::std::error::Error for E<T>\nwhere\n    T: Copy,\n{\n}\n";
        assert_sugar(
            source,
            "pub enum E<T>\nwhere\n    T: Copy,\n{\n    #[display(\"a\")]\n    A(from T),\n    \
             #[display(\"a long message about {0} and then {1}, long enough for rustfmt to break it\")]\n    \
             B(u8, u16),\n}\n",
        );
    }

    #[test]
    fn only_positional_placeholders_the_pass_writes_are_read_back() {
        assert_eq!(
            unformat("{{x}} {1:?} }} {0}", &["a", "b"]).as_deref(),
            Some("{{x}} {b:?} }} {a}")
        );
        for other in ["{}", "{name}", "{2}", "a } b", "{0"] {
            assert_eq!(unformat(other, &["a", "b"]), None, "{other}");
        }
    }
}
