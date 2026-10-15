//! `.m::<T>(..)` back to `.m<T>(..)`: the reverse of `passes/turbofish.rs` (problem 06).
//!
//! Only method calls are matched, since the pass only rewrites a `<` right after `.` and a
//! method name: path turbofish such as `Vec::<u8>::new()` or `f::<u8>()` stays. The edit removes
//! just the `::`, so comments and layout inside and around the generic arguments survive.
//!
//! A call converts only when the forward token passes read `.m<args>()` back as `.m::<args>()`,
//! asked of the passes themselves rather than of a copy of their rules: that drops an empty
//! `::<>`, which the pass leaves to the parser, and a bare std collection such as `Vec`, which
//! it fills with `_`. Calls inside a macro call's arguments stay, as for `var`: `syn` does not
//! parse macro bodies. The comma-list caveat (ARCHITECTURE h) is about comparisons read as a
//! call; the driver refuses such a file before any rule runs.

use proc_macro2::{Ident, TokenStream};
use quote::quote;
use syn::visit::{self, Visit};
use syn::{AngleBracketedGenericArguments, ExprMethodCall};

use super::{Edit, ReverseRule};

pub(crate) struct Turbofish;

impl ReverseRule for Turbofish {
    fn edits(&self, file: &syn::File, _: &str) -> Vec<Edit> {
        let mut finder = Finder { edits: Vec::new() };
        finder.visit_file(file);
        finder.edits
    }
}

struct Finder {
    edits: Vec<Edit>,
}

impl<'ast> Visit<'ast> for Finder {
    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        if let Some(turbofish) = &call.turbofish
            && let Some(colon2) = &turbofish.colon2_token
            && reads_back(&call.method, turbofish)
        {
            self.edits
                .push(Edit::between(colon2.spans[0], colon2.spans[1], ""));
        }
        visit::visit_expr_method_call(self, call);
    }
}

/// Whether the token passes turn the call's method and generic arguments, written without
/// `::`, back into the same tokens. The receiver and the call's own arguments do not take part
/// in that reading, so a placeholder and `()` stand in for them.
fn reads_back(method: &Ident, turbofish: &AngleBracketedGenericArguments) -> bool {
    let (lt, args, gt) = (&turbofish.lt_token, &turbofish.args, &turbofish.gt_token);
    let sugar: TokenStream = quote!(receiver.#method #lt #args #gt ());
    let plain = quote!(receiver.#method #turbofish ()).to_string();
    crate::passes::rewrite_tokens(sugar).is_ok_and(|tokens| tokens.to_string() == plain)
}

#[cfg(test)]
mod tests {
    use super::Turbofish;
    use crate::sugar::tests::assert_sugar;
    use crate::sugar::{ReverseRule, sugar};

    #[test]
    fn method_generics_lose_their_turbofish() {
        assert_sugar(
            "fn f(s: &str) -> Option<u32> { let n = s.parse::<u32>().ok()?; Some(n) }",
            "fn f(s: &str) -> Option<u32> { let n = s.parse<u32>().ok()?; Some(n) }",
        );
    }

    #[test]
    fn several_nested_and_filled_arguments_are_kept_as_written() {
        assert_sugar(
            "fn f() { it.collect::<HashMap<String, Vec<u8>>>(); it.unzip::<Vec<_>, Vec<u16>>(); \
             it.collect::<Vec<_>>(); x.boxed::<Box<dyn Fn() -> u8>>(); \
             x.m::<'a, 3, { N + 1 }, <T as Tr>::Out, Item = u8>(); }",
            "fn f() { it.collect<HashMap<String, Vec<u8>>>(); it.unzip<Vec<_>, Vec<u16>>(); \
             it.collect<Vec<_>>(); x.boxed<Box<dyn Fn() -> u8>>(); \
             x.m<'a, 3, { N + 1 }, <T as Tr>::Out, Item = u8>(); }",
        );
    }

    #[test]
    fn chains_and_calls_nested_in_receivers_and_arguments_each_convert() {
        assert_sugar(
            "fn f() { a.map::<u8>(g).collect::<Vec<u8>>().len(); \
             x.m::<u8>(y.n::<u16>(z.o::<u32>())); (t.0).p::<i8>(); }",
            "fn f() { a.map<u8>(g).collect<Vec<u8>>().len(); \
             x.m<u8>(y.n<u16>(z.o<u32>())); (t.0).p<i8>(); }",
        );
    }

    #[test]
    fn calls_in_closures_impls_consts_and_comma_lists_convert() {
        assert_sugar(
            "impl S { fn m(&self) -> (bool, u8) { let g = |s: &str| s.parse::<u8>(); \
             (a.b < c, d.e::<u8>(f)) } }\nconst N: usize = X.len::<u8>();\n",
            "impl S { fn m(&self) -> (bool, u8) { let g = |s: &str| s.parse<u8>(); \
             (a.b < c, d.e<u8>(f)) } }\nconst N: usize = X.len<u8>();\n",
        );
    }

    #[test]
    fn comments_and_layout_around_the_turbofish_survive() {
        assert_sugar(
            "fn f() {\n    // tags\n    let v = it\n        .collect /* a */ ::</* b */ Vec<\n            \
             String, // c\n        >>();\n}\n",
            "fn f() {\n    // tags\n    let v = it\n        .collect /* a */ </* b */ Vec<\n            \
             String, // c\n        >>();\n}\n",
        );
    }

    #[test]
    fn var_and_turbofish_convert_in_one_statement() {
        assert_sugar(
            "fn f() { let mut v = it.collect::<Vec<u8>>(); v.push(1); }",
            "fn f() { var v = it.collect<Vec<u8>>(); v.push(1); }",
        );
    }

    #[test]
    fn shapes_the_pass_does_not_read_back_stay_byte_for_byte() {
        let source = "fn f() {\n    let v = Vec::<u8>::new();\n    g::<u8>();\n    \
                      <Vec<u8>>::new();\n    x.m::<>();\n    it.collect::<Vec>();\n    \
                      it.collect::<BTreeMap>();\n    vec![it.collect::<Vec<u8>>()];\n    \
                      let n = s.parse();\n}\n";
        assert_eq!(sugar(source).unwrap(), source);
    }

    #[test]
    fn the_rule_itself_skips_calls_the_pass_would_read_differently() {
        // Checked without the driver, whose round trip would also drop these edits.
        let source = "fn f() { x.m::<>(); it.collect::<Vec>(); it.unzip::<u8, HashMap>(); }";
        let file = syn::parse_file(source).unwrap();
        assert_eq!(Turbofish.edits(&file, source), []);
    }

    #[test]
    fn a_path_named_like_a_collection_is_not_filled_so_it_converts() {
        assert_sugar(
            "fn f() { it.collect::<my::Vec>(); it.collect::<String>(); }",
            "fn f() { it.collect<my::Vec>(); it.collect<String>(); }",
        );
    }
}
