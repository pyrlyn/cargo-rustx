//! `String::from("lit")` back to `"lit"`: the reverse of `passes/string_literals.rs` (problem 15,
//! T19.5).
//!
//! The pass turns a string literal into `::std::string::String::from(lit)` where the input writes
//! the type it goes into as `String`, `Option<String>` (the argument of `Some(..)`) or
//! `Vec<String>` (the elements of `vec![..]`). The rule walks the file with the pass's own
//! `Strings` and `typed_positions`, so it looks exactly where the pass converts: `let`
//! annotations, assignments to typed locals, fields of declared structs and `*r`, arguments of
//! calls to fns of the input, and struct-literal fields. Untyped `let`s, `&str` places, method
//! arguments, `+=` and `const`/`static` items are not such places.
//!
//! Only `String::from` with one plain or raw string literal converts, in any spelling `std_paths`
//! resolves to that item (`String::from`, `std::string::String::from`, `string::String::from`
//! after `use std::string`). `"lit".to_string()`, `"lit".to_owned()` and `"lit".into()` build the
//! same `String`, but the bare literal expands to `String::from`, which is other tokens; the check
//! compares paths, deliberately not calls that happen to agree, so the driver would refuse them
//! and the rule does not propose them. A shadowed `String`, byte and C strings, and a comment
//! anywhere in the call outside the literal, which the edit would drop, keep the call as written.

use proc_macro2::Span;
use quote::{ToTokens, quote};
use syn::Expr;

use super::bounds;
use super::std_paths::StdPaths;
use super::{Edit, ReverseRule};
use crate::passes::{Strings, walk_file};
use crate::tokens::syntax::is_str_literal;

pub(crate) struct StringLiterals;

impl ReverseRule for StringLiterals {
    fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit> {
        let paths = StdPaths::of(file);
        let string_from = paths.normalise(quote!(::std::string::String::from));
        let mut edits = Vec::new();
        let mut at = |place: &mut Expr| {
            edits.extend(literal(source, &paths, &string_from, place));
            // The walk's copy of the file is only read.
            false
        };
        // The walk rewrites the file it is given; a clone keeps the spans of the original.
        walk_file(&mut file.clone(), &mut Strings(&mut at));
        edits
    }
}

/// The edit turning `place` into its literal, when it is `String::from(lit)` with nothing but
/// whitespace around the literal.
fn literal(source: &str, paths: &StdPaths, string_from: &[String], place: &Expr) -> Option<Edit> {
    let Expr::Call(call) = place else {
        return None;
    };
    let Expr::Path(func) = &*call.func else {
        return None;
    };
    let plain = call.attrs.is_empty()
        && func.attrs.is_empty()
        && func.qself.is_none()
        && call.args.len() == 1
        && is_str_literal(&call.args[0])
        && paths.normalise(func.path.to_token_stream()) == string_from;
    if !plain {
        return None;
    }
    let (start, _) = bounds(func.to_token_stream())?;
    let (first, last) = bounds(call.args[0].to_token_stream())?;
    let close = call.paren_token.span.close();
    let byte = |span: Span, end: bool| {
        let range = span.byte_range();
        if end { range.end } else { range.start }
    };
    let around = [
        byte(start, false)..byte(first, false),
        byte(last, true)..byte(close, true),
    ];
    // Outside the literal the call is path segments, `::` and parentheses: a `/` there starts a
    // comment the edit would drop, and a `,` is a trailing comma the expansion does not write.
    let bare = around
        .into_iter()
        .all(|gap| source.get(gap).is_some_and(|gap| !gap.contains(['/', ','])));
    let text = source.get(byte(first, false)..byte(last, true))?;
    bare.then(|| Edit::between(start, close, text))
}

#[cfg(test)]
mod tests {
    use super::StringLiterals;
    use crate::sugar::tests::assert_sugar;
    use crate::sugar::{ReverseRule, sugar};

    const CONTACT: &str =
        "struct Contact { name: String, email: Option<String>, tags: Vec<String> }\n";

    fn with_contact(body: &str) -> String {
        format!("{CONTACT}{body}")
    }

    /// The source comes back unchanged, and not because the driver dropped an edit.
    fn assert_no_edits(source: &str) {
        let file = syn::parse_file(source).unwrap();
        assert_eq!(StringLiterals.edits(&file, source), []);
        assert_eq!(sugar(source).unwrap(), source);
    }

    #[test]
    fn string_from_a_literal_becomes_the_literal_in_every_position_the_pass_converts() {
        assert_sugar(
            &with_contact(
                "impl Contact {\n    fn rename(&mut self) { self.name = String::from(\"anon\"); }\n    \
                 fn new() -> Self { Self { name: String::from(\"Ann\"), email: None, tags: vec![] } }\n}\n\
                 fn put(name: String, tags: Vec<String>) {}\n\
                 fn f(c: &mut Contact, r: &mut String, mut s: String) {\n    \
                 let a: String = String::from(\"a\");\n    \
                 let b: Option<String> = Some(String::from(r\"b\"));\n    \
                 let v: Vec<String> = vec![String::from(\"x\"), a.clone(), String::from(\"y\"),];\n    \
                 let w: Vec<String> = vec![String::from(\"z\"); 3];\n    \
                 s = String::from(\"s\");\n    c.email = Some(String::from(\"d\"));\n    \
                 *r = String::from(\"e\");\n    put(String::from(\"g\"), vec![String::from(\"h\")]);\n    \
                 let k = Contact { name: String::from(\"i\"), email: Some(String::from(\"j\")), tags: vec![] };\n    \
                 let m = |t: String| { let u: String = String::from(\"l\"); };\n}\n",
            ),
            &with_contact(
                "impl Contact {\n    fn rename(&mut self) { self.name = \"anon\"; }\n    \
                 fn new() -> Self { Self { name: \"Ann\", email: None, tags: vec![] } }\n}\n\
                 fn put(name: String, tags: Vec<String>) {}\n\
                 fn f(c: &mut Contact, r: &mut String, mut s: String) {\n    \
                 let a: String = \"a\";\n    \
                 let b: Option<String> = Some(r\"b\");\n    \
                 let v: Vec<String> = vec![\"x\", a.clone(), \"y\",];\n    \
                 let w: Vec<String> = vec![\"z\"; 3];\n    \
                 s = \"s\";\n    c.email = Some(\"d\");\n    \
                 *r = \"e\";\n    put(\"g\", vec![\"h\"]);\n    \
                 let k = Contact { name: \"i\", email: Some(\"j\"), tags: vec![] };\n    \
                 let m = |t: String| { let u: String = \"l\"; };\n}\n",
            ),
        );
    }

    #[test]
    fn every_spelling_the_check_equates_converts_and_layout_around_the_call_survives() {
        assert_sugar(
            "use std::string;\nfn f() {\n    let a: String = std::string::String::from(\"a\");\n    \
             let b: std::string::String = ::std::string::String::from(r#\"b\"#);\n    \
             let c: String = string::String::from(\"c\");\n    \
             let d: String = String::from(\n        \"d\"\n    );\n    \
             let e: String = String :: from ( \"e\" ) ; // kept\n}\n",
            "use std::string;\nfn f() {\n    let a: String = \"a\";\n    \
             let b: std::string::String = r#\"b\"#;\n    \
             let c: String = \"c\";\n    \
             let d: String = \"d\";\n    \
             let e: String = \"e\" ; // kept\n}\n",
        );
    }

    #[test]
    fn let_mut_and_a_literal_convert_in_one_statement() {
        assert_sugar(
            "fn f() { let mut a: String = String::from(\"a\"); a = String::from(\"b\"); }",
            "fn f() { var a: String = \"a\"; a = \"b\"; }",
        );
    }

    #[test]
    fn places_the_pass_does_not_convert_keep_the_call() {
        assert_no_edits(&with_contact(
            "struct Label { text: &'static str, alt: Option<&'static str> }\n\
             fn put(s: &str) {}\n\
             const C: String = String::from(\"c\");\n\
             impl Contact { fn rename(&mut self, name: String) {} }\n\
             fn f(c: &mut Contact, mut s: String) {\n    \
             let a = String::from(\"a\");\n    \
             let b: &str = String::from(\"b\");\n    \
             let d: Option<&str> = Some(String::from(\"d\"));\n    \
             let e: Vec<&str> = vec![String::from(\"e\")];\n    \
             let g = Label { text: String::from(\"g\"), alt: Some(String::from(\"h\")) };\n    \
             put(String::from(\"i\"));\n    c.rename(String::from(\"j\"));\n    \
             s += String::from(\"k\");\n    \
             let l = Other { name: String::from(\"l\") };\n    \
             let m: String = (String::from(\"m\"));\n    \
             let n: Option<String> = other::Some(String::from(\"n\"));\n    \
             let o: Vec<String> = my::vec![String::from(\"o\")];\n    \
             let p: my::String = String::from(\"p\");\n}\n",
        ));
    }

    #[test]
    fn qualified_some_constructors_round_trip_with_string_literals() {
        for constructor in [
            "Option::Some",
            "::core::option::Option::Some",
            "::std::option::Option::Some",
        ] {
            assert_sugar(
                &format!(
                    "fn f() {{ let n: Option<String> = {constructor}(String::from(\"n\")); }}"
                ),
                &format!("fn f() {{ let n: Option<String> = {constructor}(\"n\"); }}"),
            );
        }
    }

    #[test]
    fn other_conversions_and_values_that_are_not_str_literals_keep_their_spelling() {
        assert_no_edits(
            "fn f(name: &str) {\n    let a: String = \"a\".to_string();\n    \
             let b: String = \"b\".to_owned();\n    let c: String = \"c\".into();\n    \
             let d: String = String::from(name);\n    let e: String = String::from(b\"e\");\n    \
             let g: String = String::from(c\"g\");\n    \
             let h: String = String::from(\"h\".to_owned());\n    \
             let i: String = <String as From<&str>>::from(\"i\");\n    \
             let k: String = From::from(\"k\");\n}\n",
        );
        // Checked without the driver, which refuses to read a trailing comma in a call at all.
        let source = "fn f() { let j: String = String::from(\"j\",); }\n";
        let file = syn::parse_file(source).unwrap();
        assert_eq!(StringLiterals.edits(&file, source), []);
    }

    #[test]
    fn a_shadowed_string_keeps_the_call() {
        assert_no_edits("use my::String;\nfn f() { let a: String = String::from(\"a\"); }\n");
        assert_no_edits("struct String;\nfn f() { let a: String = String::from(\"a\"); }\n");
        assert_no_edits("fn f<String>() { let a: String = String::from(\"a\"); }\n");
        assert_no_edits("fn f() { let a: String = string::String::from(\"a\"); }\n");
    }

    #[test]
    fn a_comment_in_the_call_keeps_it() {
        assert_no_edits(
            "fn f() {\n    let a: String = String::from(/* why */ \"a\");\n    \
             let b: String = String::from(\"b\" // why\n    );\n    \
             let c: String = String:: /* why */ from(\"c\");\n}\n",
        );
    }
}
