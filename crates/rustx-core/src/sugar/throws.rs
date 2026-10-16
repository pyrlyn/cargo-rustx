//! `Result` fns back to `-> T throws E`: the reverse of `passes/throws.rs` (problem 10, T19.7).
//!
//! The pass turns `-> T throws E` into `-> ::core::result::Result<T, E>`, wraps the body's tail
//! in `::core::result::Result::Ok(..)` and appends `Ok(())` to a body without a value tail. A fn
//! returning `Result<T, E>`, in any spelling `std_paths` resolves to that item, converts when its
//! body is one the pass builds:
//!
//! - a tail `Ok(v)` keeps only `v`, unless the pass would not wrap `v`: an `Ok` or `Err` call, a
//!   `for`, `while` or `if` without `else`, or a value that never arrives (`return`, `panic!` and
//!   the like, a `loop` no `break` leaves);
//! - a tail `Ok(())` goes when the pass appends it again after what precedes it;
//! - a body ending in no value, which the pass leaves as it is, and a declaration without a body
//!   only take the new return type.
//!
//! Left alone: a tail that is any other `Result` expression (`Err(e)`, a call such as
//! `text.parse()`), `Result` with arguments other than `<T, E>`, aliases such as `io::Result<T>`,
//! and `anyhow::Result<T>`, whose plain `T throws` form names a path the check does not equate.
//! `return Err(e)` stays as written: `throw e` expands to `Err(Into::into(e))`, which is neither
//! `Err(e)` nor `Err(e.into())` token for token. A comment among the removed tokens (`Result`,
//! `<`, `,`, `>`, `Ok(`, `)`) keeps the fn as written, since it would be lost.
//!
//! The return type and the tail convert together or not at all (one edit set): the new return
//! type alone also expands back when the tail is `Ok(..)`, but it is not what the pass makes.

use std::ops::Range;

use quote::{ToTokens, quote};
use syn::visit::{self, Visit};
use syn::{
    Block, Expr, ExprCall, GenericArgument, ImplItemFn, ItemFn, PathArguments, ReturnType,
    Signature, Stmt, TraitItemFn, Type,
};

use super::rc_values::bounds;
use super::std_paths::StdPaths;
use super::{Edit, ReverseRule};
use crate::passes::throws::{diverging_macro, is_ok_or_err, is_unit, produces_no_value};

pub(crate) struct Throws;

impl ReverseRule for Throws {
    fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit> {
        self.edit_groups(file, source).concat()
    }

    fn edit_groups(&self, file: &syn::File, source: &str) -> Vec<Vec<Edit>> {
        let paths = StdPaths::of(file);
        let mut finder = Finder {
            source,
            result: paths.normalise(quote!(::core::result::Result)),
            ok: paths.normalise(quote!(::core::result::Result::Ok)),
            paths: &paths,
            groups: Vec::new(),
        };
        finder.visit_file(file);
        finder.groups
    }
}

struct Finder<'a> {
    source: &'a str,
    paths: &'a StdPaths,
    result: Vec<String>,
    ok: Vec<String>,
    groups: Vec<Vec<Edit>>,
}

impl Finder<'_> {
    fn found(&mut self, sig: &Signature, body: Option<&Block>) {
        let mut group = Vec::new();
        let converted = self.signature(sig).is_some_and(|edit| {
            group.push(edit);
            body.is_none_or(|body| self.body(body, &mut group).is_some())
        });
        if converted {
            self.groups.push(group);
        }
    }

    /// `Result<T, E>` as `T throws E`.
    fn signature(&self, sig: &Signature) -> Option<Edit> {
        let ReturnType::Type(_, ty) = &sig.output else {
            return None;
        };
        let Type::Path(ty) = &**ty else {
            return None;
        };
        let last = ty.path.segments.last()?;
        let PathArguments::AngleBracketed(args) = &last.arguments else {
            return None;
        };
        let [GenericArgument::Type(ok), GenericArgument::Type(error)] =
            &args.args.iter().collect::<Vec<_>>()[..]
        else {
            return None;
        };
        let mut path = ty.path.clone();
        path.segments.last_mut()?.arguments = PathArguments::None;
        let plain = ty.qself.is_none()
            && args.colon2_token.is_none()
            && !args.args.trailing_punct()
            && path
                .segments
                .iter()
                .all(|segment| segment.arguments.is_none());
        if !plain || self.paths.normalise(path.to_token_stream()) != self.result {
            return None;
        }
        let (whole, ok, error) = (range(ty)?, range(ok)?, range(error)?);
        let removed = [
            whole.start..ok.start,
            ok.end..error.start,
            error.end..whole.end,
        ];
        let text = format!(
            "{} throws {}",
            self.source.get(ok)?,
            self.source.get(error)?
        );
        self.no_comment(removed)
            .then_some(Edit { range: whole, text })
    }

    /// Adds the edits that leave `body` as the pass would have received it; `None` when the
    /// pass would not build this body.
    fn body(&self, body: &Block, edits: &mut Vec<Edit>) -> Option<()> {
        let (last, before) = body.stmts.split_last()?;
        let keeps = match last {
            Stmt::Expr(tail, None) => match self.ok_call(tail) {
                Some((call, Expr::Tuple(unit)))
                    if unit.elems.is_empty() && unit.attrs.is_empty() =>
                {
                    return self.drop_ok_unit(call, before, edits);
                }
                Some((call, value)) => return self.unwrap(call, value, edits),
                None => !is_unit(tail) && produces_no_value(tail),
            },
            Stmt::Expr(last, Some(_)) => produces_no_value(last),
            Stmt::Macro(last) => diverging_macro(&last.mac),
            Stmt::Local(_) | Stmt::Item(_) => false,
        };
        keeps.then_some(())
    }

    /// `call` when it calls `Ok` with one argument, through a plain path.
    fn ok_call<'e>(&self, expr: &'e Expr) -> Option<(&'e ExprCall, &'e Expr)> {
        let Expr::Call(call) = expr else {
            return None;
        };
        let Expr::Path(func) = &*call.func else {
            return None;
        };
        let plain = call.attrs.is_empty()
            && func.attrs.is_empty()
            && func.qself.is_none()
            && func.path.segments.iter().all(|s| s.arguments.is_none())
            && call.args.len() == 1
            && !call.args.trailing_punct();
        (plain && self.paths.normalise(func.path.to_token_stream()) == self.ok)
            .then(|| (call, &call.args[0]))
    }

    /// `Ok(value)` as `value`, where the pass wraps that tail again.
    fn unwrap(&self, call: &ExprCall, value: &Expr, edits: &mut Vec<Edit>) -> Option<()> {
        if is_unit(value) || produces_no_value(value) || is_ok_or_err(value) {
            return None;
        }
        let (call, value) = (range(call)?, range(value)?);
        // Two edits around the value rather than one over it, so other rules' edits inside the
        // value do not overlap this one.
        let removed = [call.start..value.start, value.end..call.end];
        edits.extend(removed.iter().map(|range| Edit {
            range: range.clone(),
            text: String::new(),
        }));
        self.no_comment(removed).then_some(())
    }

    /// Removes a tail `Ok(())` with the space before it, where the pass appends it again.
    fn drop_ok_unit(&self, call: &ExprCall, before: &[Stmt], edits: &mut Vec<Edit>) -> Option<()> {
        // What the pass does to a body that ends with `before`.
        let appends = match before.last() {
            Some(Stmt::Expr(last, None)) => is_unit(last),
            Some(Stmt::Expr(last, Some(_))) => !produces_no_value(last),
            Some(Stmt::Macro(last)) => !diverging_macro(&last.mac),
            Some(Stmt::Local(_) | Stmt::Item(_)) | None => true,
        };
        let call = range(call)?;
        let start = self.source.get(..call.start)?.trim_end().len();
        edits.push(Edit {
            range: start..call.end,
            text: String::new(),
        });
        (appends && self.no_comment([call])).then_some(())
    }

    /// Whether the source in `ranges`, which holds only paths and punctuation, has no comment.
    fn no_comment(&self, ranges: impl IntoIterator<Item = Range<usize>>) -> bool {
        ranges.into_iter().all(|range| {
            self.source
                .get(range)
                .is_some_and(|text| !text.contains('/'))
        })
    }
}

/// The bytes of the source that `node` was parsed from.
fn range(node: &impl ToTokens) -> Option<Range<usize>> {
    let (first, last) = bounds(node.to_token_stream())?;
    Some(first.byte_range().start..last.byte_range().end)
}

impl<'ast> Visit<'ast> for Finder<'_> {
    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        self.found(&item.sig, Some(&item.block));
        visit::visit_item_fn(self, item);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        self.found(&item.sig, Some(&item.block));
        visit::visit_impl_item_fn(self, item);
    }

    fn visit_trait_item_fn(&mut self, item: &'ast TraitItemFn) {
        self.found(&item.sig, item.default.as_ref());
        visit::visit_trait_item_fn(self, item);
    }
}

#[cfg(test)]
mod tests {
    use super::Throws;
    use crate::sugar::tests::assert_sugar;
    use crate::sugar::{Edit, ReverseRule, convert, sugar};

    /// The rule's own edits for `source`, before the driver checks them.
    fn edits(source: &str) -> Vec<Edit> {
        Throws.edits(&syn::parse_file(source).unwrap(), source)
    }

    #[test]
    fn a_result_fn_with_an_ok_tail_becomes_throws() {
        assert_sugar(
            "fn parse(text: &str) -> Result<u32, ParseError> {\n    let n = text.parse()?;\n    \
             Ok(n + 1)\n}\n",
            "fn parse(text: &str) -> u32 throws ParseError {\n    let n = text.parse()?;\n    \
             n + 1\n}\n",
        );
    }

    #[test]
    fn every_fn_kind_and_spelling_converts() {
        assert_sugar(
            "use std::error::Error;\n\
             fn a<T: Clone>(t: &T) -> std::result::Result<Vec<T>, Box<dyn Error + Send + Sync>>\n\
             where\n    T: Default,\n{\n    ::core::result::Result::Ok(vec![t.clone()])\n}\n\
             impl S {\n    pub async fn b(&self) -> Result<u8, E> { Ok(match self.0 { 0 => 1, n => n }) }\n}\n\
             trait T {\n    fn c(&self) -> Result<u8, E>;\n    fn d(&self) -> Result<u8, E> {\n        \
             fn inner() -> Result<u8, E> { Ok(1) }\n        Ok(self.c()? + inner()?)\n    }\n}\n",
            "use std::error::Error;\n\
             fn a<T: Clone>(t: &T) -> Vec<T> throws Box<dyn Error + Send + Sync>\n\
             where\n    T: Default,\n{\n    vec![t.clone()]\n}\n\
             impl S {\n    pub async fn b(&self) -> u8 throws E { match self.0 { 0 => 1, n => n } }\n}\n\
             trait T {\n    fn c(&self) -> u8 throws E;\n    fn d(&self) -> u8 throws E {\n        \
             fn inner() -> u8 throws E { 1 }\n        self.c()? + inner()?\n    }\n}\n",
        );
    }

    #[test]
    fn a_tail_ok_unit_goes_where_the_pass_appends_it() {
        assert_sugar(
            "fn a(xs: &[u8]) -> Result<(), E> {\n    for x in xs {\n        step(*x)?;\n    }\n    \
             Ok(())\n}\n\
             fn b() -> Result<(), E> {\n    run()?;\n    // done\n    Ok(())\n}\n\
             fn c() -> Result<(), E> {\n    Ok(())\n}\n",
            "fn a(xs: &[u8]) -> () throws E {\n    for x in xs {\n        step(*x)?;\n    }\n}\n\
             fn b() -> () throws E {\n    run()?;\n    // done\n}\n\
             fn c() -> () throws E {\n}\n",
        );
    }

    #[test]
    fn a_body_without_a_value_only_takes_the_return_type() {
        assert_sugar(
            "fn a() -> Result<u8, E> { todo!() }\nfn b() -> Result<u8, E> { loop { tick()?; } }\n\
             fn c(x: u8) -> Result<u8, E> { if x > 0 { return Ok(x); } panic!(\"zero\"); }\n",
            "fn a() -> u8 throws E { todo!() }\nfn b() -> u8 throws E { loop { tick()?; } }\n\
             fn c(x: u8) -> u8 throws E { if x > 0 { return Ok(x); } panic!(\"zero\"); }\n",
        );
    }

    #[test]
    fn other_rules_convert_inside_a_throws_fn() {
        assert_sugar(
            "fn f(xs: &[u8]) -> Result<Vec<u8>, E> {\n    let mut n = 0;\n    n += 1;\n    \
             Ok(xs.iter().map(|x| x + n).collect::<Vec<u8>>())\n}\n",
            "fn f(xs: &[u8]) -> Vec<u8> throws E {\n    var n = 0;\n    n += 1;\n    \
             xs.iter().map(|x| x + n).collect<Vec<u8>>()\n}\n",
        );
    }

    #[test]
    fn fns_the_pass_would_not_produce_stay_plain() {
        let source = "fn a() -> Result<u8, E> { Err(E) }\n\
             fn b() -> Result<u8, E> { text.parse() }\n\
             fn c() -> Result<Result<u8, E>, E> { Ok(Ok(1)) }\n\
             fn d() -> Result<u8, E> { Ok(return Ok(1)) }\n\
             fn e(o: u8) -> Result<(), E> { match o { _ => {} } Ok(()) }\n\
             fn f() -> Result<(), E> { loop {} Ok(()) }\n\
             fn g() -> Result<u8, E> { let x = 1; }\n\
             fn h() -> Result<u8> { Ok(1) }\n\
             fn i() -> io::Result<u8> { Ok(1) }\n\
             fn j() -> anyhow::Result<u8> { Ok(1) }\n\
             fn k() -> Result<u8, E,> { Ok(1) }\n\
             fn l() -> Result<u8, E> { Ok::<u8, E>(1) }\n\
             fn m() -> Result<u8 /* why */, E> { Ok(1) }\n\
             fn n() -> Result<u8, E> { Ok(/* why */ 1) }\n\
             fn o() -> Result<u8, E> { #[allow(unused)] Ok(1) }\n\
             fn p() -> Option<u8> { Some(1) }\n\
             fn q() { let c = || -> Result<u8, E> { Ok(1) }; }\n";
        assert_eq!(edits(source), []);
        assert_eq!(sugar(source).unwrap(), source);
    }

    #[test]
    fn a_shadowed_result_or_ok_stays_plain() {
        let shadowed = [
            "use my::Result;\nfn f() -> Result<u8, E> { Ok(1) }\n",
            "type Result<T, E> = Vec<(T, E)>;\nfn f() -> Result<u8, E> { Ok(1) }\n",
            "fn f<Result>() -> Result<u8, E> { Ok(1) }\n",
            "struct Ok<T>(T);\nfn f() -> Result<u8, E> { Ok(1) }\n",
            "use my::*;\nfn f() -> Result<u8, E> { Ok(1) }\n",
        ];
        for source in shadowed {
            assert_eq!(edits(source), [], "{source}");
            assert_eq!(sugar(source).unwrap(), source);
        }
    }

    /// The same edits, each of them a set of its own.
    struct Apart;
    impl ReverseRule for Apart {
        fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit> {
            Throws.edits(file, source)
        }
    }

    #[test]
    fn the_return_type_never_converts_without_its_tail() {
        // As a tail, `m! { 1 }` is a statement, after which the pass appends `Ok(())`. The new
        // return type alone would expand back, since the pass leaves an `Ok(..)` tail alone.
        let source = "fn f() -> Result<u8, E> { Ok(m! { 1 }) }\n";
        assert_eq!(edits(source).len(), 3);
        assert_eq!(sugar(source).unwrap(), source);

        assert_eq!(
            convert(source, &[&Apart]).unwrap(),
            "fn f() -> u8 throws E { Ok(m! { 1 }) }\n"
        );
    }
}
