//! `x: Option<T>` unwrapped by `let x: T = x.unwrap_or(v);` back to the parameter default
//! `x: T = v`, with the calls in the file back to default and named arguments: the reverse of
//! `passes/param_defaults.rs` and `passes/call_defaults.rs` (problem 17, T19.8).
//!
//! A fn converts when its body starts with exactly the statements `param_defaults` emits for a
//! suffix of its parameters, in parameter order: `let x: T = x.unwrap_or(v);` for a `v` the pass
//! counts as constant, `let x: T = x.unwrap_or_else(|| v);` for any other, `let mut x` for a
//! `mut` binding, and no annotation exactly when `T` holds `impl Trait`. The parameter must be a
//! plain name of type `Option<T>`, in any spelling `std_paths` resolves to the std item.
//!
//! As sugar, `call_defaults` rewrites every call it resolves to such a fn, so the calls come back
//! too. They are resolved with that pass's own `Fns`; each must pass every argument, with `None`
//! or `Some(e)` at the defaulted positions. Trailing `None`s are dropped, `Some(e)` becomes `e`,
//! and after a `None` that cannot be dropped the given values are named (`f(a, y: 2)`). If one
//! call to a name cannot be written back, no fn of that name converts and none of its calls do:
//! a definition converted without its calls, or calls without their definition, expand to other
//! code.
//!
//! The edits of a fn and its calls stay separate, so other rules still edit the code between
//! them. They are kept or dropped together by the driver's check alone: every proper subset
//! expands to other code (a converted fn wraps a plain call's `Some(e)` in `Some` again; a plain
//! fn leaves a converted call short of arguments). A call inside a default moves into the
//! parameter with it, converted in the moved text. An edit that would delete a comment, or
//! inner attributes before the unwrapping statements, is not made.

use std::collections::HashSet;
use std::ops::Range;

use proc_macro2::extra::DelimSpan;
use proc_macro2::{Ident, Span};
use quote::{ToTokens, quote};
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{
    Attribute, Block, Expr, ExprCall, ExprMethodCall, FnArg, GenericArgument, ImplItemFn, ItemFn,
    ItemImpl, ItemTrait, Macro, Pat, PatIdent, PatType, PathArguments, Signature, Stmt, Token,
    TraitItemFn, Type,
};

use super::bounds;
use super::std_paths::StdPaths;
use super::{Edit, ReverseRule, apply};
use crate::passes::{
    DEFAULT_MARKER, Fns, MacroArgs, Shape, has_impl_trait, is_constant, path_target, type_name,
};

pub(crate) struct ParamDefaults;

impl ReverseRule for ParamDefaults {
    fn edits(&self, file: &syn::File, source: &str) -> Vec<Edit> {
        let paths = StdPaths::of(file);
        let mut found = Found {
            source,
            option: paths.normalise(quote!(::core::option::Option)),
            some: paths.normalise(quote!(::core::option::Option::Some)),
            none: paths.normalise(quote!(::core::option::Option::None)),
            paths: &paths,
            collecting: true,
            owner: None,
            fns: Fns::default(),
            defs: Vec::new(),
            calls: Vec::new(),
            refused: HashSet::new(),
        };
        found.visit_file(file);
        // A call may come before the fn it calls, so every signature is known before any call.
        found.collecting = false;
        found.visit_file(file);
        found.finish()
    }
}

/// A fn whose leading statements unwrap its trailing `Option` parameters.
struct Definition {
    name: String,
    params: Vec<Param>,
    /// From the end of the body's `{` to the end of the last unwrapping statement.
    lets: Range<usize>,
}

/// `x: Option<T>` becomes `head` followed by the default value, taken from `value`.
struct Param {
    range: Range<usize>,
    head: String,
    value: Range<usize>,
}

/// The parts of one `let x: T = x.unwrap_or(v);` statement.
struct Unwrap<'a> {
    name: &'a Ident,
    pat: &'a Pat,
    value: &'a Expr,
}

/// What a call passes for one parameter.
enum Arg<'a> {
    Plain,
    None,
    Some(&'a ExprCall),
}

struct Found<'a> {
    source: &'a str,
    paths: &'a StdPaths,
    option: Vec<String>,
    some: Vec<String>,
    none: Vec<String>,
    collecting: bool,
    /// The type of the enclosing `impl` or trait, as `call_defaults` tracks it.
    owner: Option<String>,
    fns: Fns,
    defs: Vec<Definition>,
    /// The edits of every call that resolves to a fn with a default, by the fn's name.
    calls: Vec<(String, Vec<Edit>)>,
    /// Names with a call that cannot be written back, or that resolves ambiguously.
    refused: HashSet<String>,
}

impl Found<'_> {
    fn signature(&mut self, sig: &Signature, body: Option<&Block>, associated: bool) {
        if !self.collecting {
            return;
        }
        let owner = associated.then(|| self.owner.clone().unwrap_or_default());
        match body.and_then(|body| self.definition(sig, body)) {
            Some((def, marked)) => {
                self.fns.add(&marked, owner);
                self.defs.push(def);
            }
            None => self.fns.add(sig, owner),
        }
    }

    /// The conversion of a fn, and its signature as `call_defaults` would see the sugar.
    fn definition(&self, sig: &Signature, body: &Block) -> Option<(Definition, Signature)> {
        let typed: Vec<&PatType> = sig
            .inputs
            .iter()
            .filter_map(|arg| match arg {
                FnArg::Typed(param) => Some(param),
                FnArg::Receiver(_) => None,
            })
            .collect();
        let mut run = Vec::new();
        for stmt in &body.stmts {
            let Some(unwrap) = unwrap(stmt) else { break };
            let Some(at) = typed
                .iter()
                .position(|param| plain_name(param) == Some(unwrap.name))
            else {
                break;
            };
            run.push((at, stmt, unwrap));
        }
        // The pass unwraps a suffix of the parameters, in order, at the start of the body.
        let count = (1..=run.len()).rev().find(|&count| {
            run[..count]
                .iter()
                .enumerate()
                .all(|(i, (at, ..))| *at + count == typed.len() + i)
        })?;
        let run = &run[..count];

        let open = body.brace_token.span.open().byte_range().end;
        let mut params = Vec::with_capacity(count);
        let mut at = open;
        for (index, stmt, unwrap) in run {
            params.push(self.param(typed[*index], (stmt, unwrap))?);
            let stmt = range(*stmt)?;
            if !self.blank(at..stmt.start) {
                return None;
            }
            at = stmt.end;
        }
        // `call_defaults` finds a default by its marker, so the sugar's signature carries one.
        let marker = Ident::new(DEFAULT_MARKER, Span::call_site());
        let attr: Attribute = syn::parse_quote!(#[#marker]);
        let mut as_sugar = sig.clone();
        let typed_rev = as_sugar
            .inputs
            .iter_mut()
            .rev()
            .filter_map(|arg| match arg {
                FnArg::Typed(param) => Some(param),
                FnArg::Receiver(_) => None,
            });
        for param in typed_rev.take(count) {
            param.attrs.push(attr.clone());
        }
        let def = Definition {
            name: sig.ident.to_string(),
            params,
            lets: open..at,
        };
        Some((def, as_sugar))
    }

    /// The edit of one parameter, when `unwrap` (the statement `stmt`) is what the pass emits.
    fn param(&self, param: &PatType, (stmt, unwrap): (&Stmt, &Unwrap)) -> Option<Param> {
        let ty = self.option_of(&param.ty)?;
        // `impl Trait` is allowed inside `Option<..>` in argument position, but not in a `let`.
        let annotation = (!has_impl_trait(ty)).then(|| quote!(: #ty));
        let (name, pat, value) = (unwrap.name, unwrap.pat, unwrap.value);
        let call = if is_constant(value) {
            quote!(#name.unwrap_or(#value))
        } else {
            quote!(#name.unwrap_or_else(|| #value))
        };
        // Parsed like the pass's output, so `||` compares as the closure prints it.
        let expected: Stmt = syn::parse2(quote!(let #pat #annotation = #call;)).ok()?;
        if stmt.to_token_stream().to_string() != expected.to_token_stream().to_string() {
            return None;
        }

        let stmt = range(stmt)?;
        let value = range(value)?;
        let ty_range = range(ty)?;
        let whole = range(&param.pat)?.start..range(&param.ty)?.end;
        // Only the value and `T` are carried over; a comment anywhere else would be lost.
        if !self.no_comment(stmt.start..value.start)
            || !self.no_comment(value.end..stmt.end)
            || !self.no_comment(whole.start..ty_range.start)
            || !self.no_comment(ty_range.end..whole.end)
        {
            return None;
        }
        let head = format!("{}: {} = ", self.text(range(pat)?)?, self.text(ty_range)?);
        Some(Param {
            range: whole,
            head,
            value,
        })
    }

    /// `T` of a type written `Option<T>`, with `Option` naming the std item.
    fn option_of<'t>(&self, ty: &'t Type) -> Option<&'t Type> {
        let Type::Path(path) = ty else { return None };
        let last = path.path.segments.last()?;
        let PathArguments::AngleBracketed(args) = &last.arguments else {
            return None;
        };
        let [GenericArgument::Type(inner)] = args.args.iter().collect::<Vec<_>>()[..] else {
            return None;
        };
        let mut bare = path.path.clone();
        bare.segments.last_mut()?.arguments = PathArguments::None;
        (path.qself.is_none()
            && !args.args.trailing_punct()
            && self.paths.normalise(bare.to_token_stream()) == self.option)
            .then_some(inner)
    }

    fn path_call(&mut self, call: &ExprCall) {
        let Some((name, qualifier)) = path_target(&call.func, self.owner.as_deref()) else {
            return;
        };
        let name = name.to_string();
        let shapes: Vec<Shape> = self
            .fns
            .path_call(&name, qualifier.as_deref())
            .into_iter()
            .cloned()
            .collect();
        let generic = matches!(&*call.func, Expr::Path(path)
            if path.path.segments.last().is_some_and(|last| !last.arguments.is_none()));
        self.call(name, &shapes, &call.args, &call.paren_token.span, generic);
    }

    fn method_call(&mut self, call: &ExprMethodCall) {
        let name = call.method.to_string();
        let shapes: Vec<Shape> = self.fns.method_call(&name).into_iter().cloned().collect();
        let generic = call.turbofish.is_some();
        self.call(name, &shapes, &call.args, &call.paren_token.span, generic);
    }

    fn call(
        &mut self,
        name: String,
        shapes: &[Shape],
        args: &Punctuated<Expr, Token![,]>,
        parens: &DelimSpan,
        generic: bool,
    ) {
        if !shapes.iter().any(Shape::has_default) {
            return;
        }
        // `call_defaults` reports candidates that read a call differently; equal shapes cannot.
        let edits = shapes
            .windows(2)
            .all(|pair| pair[0] == pair[1])
            .then(|| self.call_edits(&shapes[0], args, parens, generic))
            .flatten();
        match edits {
            Some(edits) => self.calls.push((name, edits)),
            None => {
                self.refused.insert(name);
            }
        }
    }

    /// The edits turning a call that passes every argument into one with defaults left out.
    fn call_edits(
        &self,
        shape: &Shape,
        args: &Punctuated<Expr, Token![,]>,
        parens: &DelimSpan,
        generic: bool,
    ) -> Option<Vec<Edit>> {
        let params = &shape.params;
        // `call_defaults` rebuilds the arguments it fills, without a trailing comma.
        if args.trailing_punct() {
            return None;
        }
        let args: Vec<&Expr> = args.iter().collect();
        if args.len() != params.len() {
            return None;
        }
        let kinds = params
            .iter()
            .zip(&args)
            .map(|(param, arg)| {
                if param.default {
                    self.option_arg(arg)
                } else {
                    Some(Arg::Plain)
                }
            })
            .collect::<Option<Vec<_>>>()?;
        let kept = kinds
            .iter()
            .rposition(|kind| !matches!(kind, Arg::None))
            .map_or(0, |last| last + 1);
        let mut edits = Vec::new();
        // After a `None` that stays in place, a positional value would fill that parameter.
        let mut named = false;
        for (at, kind) in kinds.iter().enumerate().take(kept) {
            match kind {
                Arg::Plain => {}
                Arg::None => {
                    named = true;
                    let next = range(args[at + 1])?.start;
                    edits.push(self.edit(range(args[at])?.start..next, "")?);
                }
                Arg::Some(call) => {
                    let value = range(&call.args[0])?;
                    let whole = range(*call)?;
                    let prefix = if named {
                        format!("{}: ", params[at].name.as_ref()?)
                    } else {
                        String::new()
                    };
                    edits.push(self.edit(whole.start..value.start, &prefix)?);
                    edits.push(self.edit(value.end..whole.end, "")?);
                }
            }
        }
        // A call with generic arguments takes no named ones (`call_defaults`, limitations).
        if named && generic {
            return None;
        }
        if kept < args.len() {
            let dropped = match kept {
                0 => parens.open().byte_range().end..parens.close().byte_range().start,
                _ => range(args[kept - 1])?.end..range(*args.last()?)?.end,
            };
            edits.push(self.edit(dropped, "")?);
        }
        Some(edits)
    }

    /// `None` or `Some(e)` with the std meaning, as an argument at a defaulted position.
    fn option_arg<'e>(&self, arg: &'e Expr) -> Option<Arg<'e>> {
        let names = |func: &Expr, expected: &[String]| {
            matches!(func, Expr::Path(path)
                if path.attrs.is_empty()
                    && path.qself.is_none()
                    && path.path.segments.iter().all(|segment| segment.arguments.is_none())
                    && self.paths.normalise(path.path.to_token_stream()) == expected)
        };
        match arg {
            Expr::Path(_) if names(arg, &self.none) => Some(Arg::None),
            Expr::Call(call)
                if call.attrs.is_empty()
                    && call.args.len() == 1
                    && !call.args.trailing_punct()
                    && names(&call.func, &self.some) =>
            {
                Some(Arg::Some(call))
            }
            _ => None,
        }
    }

    /// Every edit of the names whose fn and calls all convert.
    fn finish(self) -> Vec<Edit> {
        let mut calls: Vec<Option<Edit>> = self
            .calls
            .into_iter()
            .filter(|(name, _)| !self.refused.contains(name))
            .flat_map(|(_, edits)| edits)
            .map(Some)
            .collect();
        let mut edits = Vec::new();
        for def in self
            .defs
            .iter()
            .filter(|def| !self.refused.contains(&def.name))
        {
            for param in &def.params {
                // Calls inside the default move with it, so they are converted in its text.
                let mut inner: Vec<Edit> = calls
                    .iter_mut()
                    .filter(|edit| {
                        edit.as_ref()
                            .is_some_and(|edit| inside(&edit.range, &param.value))
                    })
                    .filter_map(Option::take)
                    .map(|edit| Edit {
                        range: edit.range.start - param.value.start
                            ..edit.range.end - param.value.start,
                        text: edit.text,
                    })
                    .collect();
                inner.sort_by_key(|edit| edit.range.start);
                let value = &self.source[param.value.clone()];
                edits.push(Edit {
                    range: param.range.clone(),
                    text: format!("{}{}", param.head, apply(value, &inner)),
                });
            }
            edits.push(Edit {
                range: def.lets.clone(),
                text: String::new(),
            });
        }
        edits.extend(calls.into_iter().flatten());
        edits
    }

    fn text(&self, range: Range<usize>) -> Option<&str> {
        self.source.get(range)
    }

    fn blank(&self, range: Range<usize>) -> bool {
        self.text(range).is_some_and(|text| text.trim().is_empty())
    }

    /// Whether `range` holds no comment; it holds no string literal, so `/` starts one.
    fn no_comment(&self, range: Range<usize>) -> bool {
        self.text(range).is_some_and(|text| !text.contains('/'))
    }

    /// An edit replacing `range`, whose text holds no comment, with `text`.
    fn edit(&self, range: Range<usize>, text: &str) -> Option<Edit> {
        self.no_comment(range.clone()).then(|| Edit {
            range,
            text: text.to_owned(),
        })
    }
}

/// The parts of `let x[: T] = x.unwrap_or(..)` / `unwrap_or_else(|| ..)`; `Found::param` checks
/// the whole statement against what the pass emits.
fn unwrap(stmt: &Stmt) -> Option<Unwrap<'_>> {
    let Stmt::Local(local) = stmt else {
        return None;
    };
    let pat = match &local.pat {
        Pat::Type(typed) => &*typed.pat,
        pat => pat,
    };
    let Pat::Ident(PatIdent {
        by_ref: None,
        subpat: None,
        ident,
        ..
    }) = pat
    else {
        return None;
    };
    let Expr::MethodCall(call) = &*local.init.as_ref()?.expr else {
        return None;
    };
    let value = match (call.method.to_string().as_str(), call.args.first()?) {
        ("unwrap_or", value) => value,
        ("unwrap_or_else", Expr::Closure(closure)) => &closure.body,
        _ => return None,
    };
    Some(Unwrap {
        name: ident,
        pat,
        value,
    })
}

/// The name of a parameter written as a plain name, as `param_defaults` keeps it.
fn plain_name(param: &PatType) -> Option<&Ident> {
    match &*param.pat {
        Pat::Ident(PatIdent {
            by_ref: None,
            mutability: None,
            subpat: None,
            ident,
            ..
        }) => Some(ident),
        _ => None,
    }
}

/// The source bytes `tokens` were parsed from.
fn range(tokens: &impl ToTokens) -> Option<Range<usize>> {
    let (first, last) = bounds(tokens.to_token_stream())?;
    Some(first.byte_range().start..last.byte_range().end)
}

fn inside(inner: &Range<usize>, outer: &Range<usize>) -> bool {
    outer.start <= inner.start && inner.end <= outer.end
}

impl<'ast> Visit<'ast> for Found<'_> {
    fn visit_item_impl(&mut self, item: &'ast ItemImpl) {
        let outer = self.owner.replace(type_name(&item.self_ty));
        visit::visit_item_impl(self, item);
        self.owner = outer;
    }

    fn visit_item_trait(&mut self, item: &'ast ItemTrait) {
        let outer = self.owner.replace(item.ident.to_string());
        visit::visit_item_trait(self, item);
        self.owner = outer;
    }

    fn visit_item_fn(&mut self, item: &'ast ItemFn) {
        self.signature(&item.sig, Some(&item.block), false);
        visit::visit_item_fn(self, item);
    }

    fn visit_impl_item_fn(&mut self, item: &'ast ImplItemFn) {
        self.signature(&item.sig, Some(&item.block), true);
        visit::visit_impl_item_fn(self, item);
    }

    fn visit_trait_item_fn(&mut self, item: &'ast TraitItemFn) {
        self.signature(&item.sig, item.default.as_ref(), true);
        visit::visit_trait_item_fn(self, item);
    }

    fn visit_expr_call(&mut self, call: &'ast ExprCall) {
        visit::visit_expr_call(self, call);
        if !self.collecting {
            self.path_call(call);
        }
    }

    fn visit_expr_method_call(&mut self, call: &'ast ExprMethodCall) {
        visit::visit_expr_method_call(self, call);
        if !self.collecting {
            self.method_call(call);
        }
    }

    /// `call_defaults` records no signature in a macro body, and rewrites calls only in bodies
    /// made of expressions.
    fn visit_macro(&mut self, mac: &'ast Macro) {
        if self.collecting {
            return;
        }
        if let Ok(mut body) = mac.parse_body::<MacroArgs>() {
            for expr in body.exprs_mut() {
                Visit::visit_expr(self, expr);
            }
        }
    }

    /// Attributes belong to whoever reads them; `call_defaults` leaves them alone too.
    fn visit_attribute(&mut self, _: &'ast Attribute) {}
}

#[cfg(test)]
mod tests {
    use super::ParamDefaults;
    use crate::sugar::tests::assert_sugar;
    use crate::sugar::{Edit, ReverseRule, convert, sugar};

    const CONNECT: &str = "fn connect(host: &str, port: Option<u16>, timeout_ms: Option<u64>) -> String {\n    \
                           let port: u16 = port.unwrap_or(5432);\n    \
                           let timeout_ms: u64 = timeout_ms.unwrap_or(5_000);\n    \
                           format!(\"{host}:{port} ({timeout_ms} ms)\")\n}\n";

    const CONNECT_SUGAR: &str = "fn connect(host: &str, port: u16 = 5432, timeout_ms: u64 = 5_000) -> String {\n    \
                                 format!(\"{host}:{port} ({timeout_ms} ms)\")\n}\n";

    /// Proposes fixed edits, whatever the file.
    struct Fixed(Vec<Edit>);

    impl ReverseRule for Fixed {
        fn edits(&self, _: &syn::File, _: &str) -> Vec<Edit> {
            self.0.clone()
        }
    }

    #[test]
    fn a_definition_converts_together_with_its_calls() {
        assert_sugar(
            &format!(
                "{CONNECT}fn demo() {{\n    connect(\"db\", None, None);\n    \
                 connect(\"db\", Some(6432), None);\n    connect(\"db\", None, Some(100));\n    \
                 connect(\"db\", Some(1), Some(2));\n    \
                 println!(\"{{}}\", connect(\"x\", ::core::option::Option::None, None));\n}}\n"
            ),
            &format!(
                "{CONNECT_SUGAR}fn demo() {{\n    connect(\"db\");\n    connect(\"db\", 6432);\n    \
                 connect(\"db\", timeout_ms: 100);\n    connect(\"db\", 1, 2);\n    \
                 println!(\"{{}}\", connect(\"x\"));\n}}\n"
            ),
        );
    }

    #[test]
    fn a_definition_without_calls_converts_alone() {
        assert_sugar(CONNECT, CONNECT_SUGAR);
    }

    #[test]
    fn lazy_and_mut_defaults_methods_and_calls_inside_defaults_convert() {
        assert_sugar(
            "struct Pool;\nimpl Pool {\n    fn new(size: Option<usize>) -> Self {\n        \
             let size: usize = size.unwrap_or(4);\n        Pool\n    }\n    \
             fn spawn(&self, n: Option<usize>, name: Option<String>) {\n        \
             let mut n: usize = n.unwrap_or(1);\n        \
             let name: String = name.unwrap_or_else(|| n.to_string());\n        n += name.len();\n    }\n    \
             fn make() -> Self { Self::new(None) }\n}\n\
             fn base(scale: Option<u64>) -> u64 {\n    let scale: u64 = scale.unwrap_or(1);\n    scale\n}\n\
             fn wait(ms: Option<u64>) -> u64 {\n    let ms: u64 = ms.unwrap_or_else(|| base(None) * 10);\n    \
             base(Some(base(Some(ms))))\n}\n\
             fn any(s: Option<impl Default>) {\n    let s = s.unwrap_or_else(|| Default::default());\n}\n\
             fn demo() { Pool::new(Some(8)).spawn(None, Some(String::new())); any::<u8>(None); }\n",
            "struct Pool;\nimpl Pool {\n    fn new(size: usize = 4) -> Self {\n        Pool\n    }\n    \
             fn spawn(&self, mut n: usize = 1, name: String = n.to_string()) {\n        n += name.len();\n    }\n    \
             fn make() -> Self { Self::new() }\n}\n\
             fn base(scale: u64 = 1) -> u64 {\n    scale\n}\n\
             fn wait(ms: u64 = base() * 10) -> u64 {\n    base(base(ms))\n}\n\
             fn any(s: impl Default = Default::default()) {\n}\n\
             fn demo() { Pool::new(8).spawn(name: String::new()); any::<u8>(); }\n",
        );
    }

    #[test]
    fn other_option_parameters_stay_plain() {
        let source = "fn a(x: Option<u8>) -> u8 { x.unwrap_or(1) }\n\
                      fn b(x: Option<u8>) { let y: u8 = x.unwrap_or(1); }\n\
                      fn c(x: Option<u8>, y: u8) { let x: u8 = x.unwrap_or(1); }\n\
                      fn d(x: Option<u8>) { let x: u8 = x.unwrap_or_else(|| 1); }\n\
                      fn e(x: Option<u8>) { let x: u8 = x.unwrap_or(f()); }\n\
                      fn g(x: Option<u8>) { let x: u16 = x.unwrap_or(1); }\n\
                      fn h(x: Option<u8>) { let x = x.unwrap_or(1); }\n\
                      fn i(x: Option<u8>) {\n    // why\n    let x: u8 = x.unwrap_or(1);\n}\n\
                      fn j(x: Option<u8>) { let x: u8 = x.unwrap_or(/* why */ 1); }\n\
                      fn k(x: Option<u8>) { let x: u8 = x.unwrap_or(1); }\n\
                      fn l(mut x: Option<u8>) { let x: u8 = x.unwrap_or(1); }\n\
                      fn demo(v: Option<u8>) { k(v); }\n\
                      trait T { fn t(&self, x: Option<u8>); }\n";
        assert_eq!(sugar(source).unwrap(), source);
        let file = syn::parse_file(source).unwrap();
        assert_eq!(ParamDefaults.edits(&file, source), []);
    }

    #[test]
    fn shadowed_option_some_or_none_stay_plain() {
        let unwraps = "fn f(x: Option<u8>) { let x: u8 = x.unwrap_or(1); }\n";
        for source in [
            format!("struct Option<T>(T);\n{unwraps}"),
            format!("use crate::maybe::None;\n{unwraps}fn g() {{ f(None); }}\n"),
            format!("use crate::maybe::Some;\n{unwraps}fn g() {{ f(Some(1)); }}\n"),
        ] {
            assert_eq!(sugar(&source).unwrap(), source);
            let file = syn::parse_file(&source).unwrap();
            assert_eq!(ParamDefaults.edits(&file, &source), []);
        }
    }

    #[test]
    fn calls_that_cannot_be_written_back_keep_the_definition_plain() {
        let source = format!(
            "{CONNECT}fn demo(p: Option<u16>) {{\n    connect(\"db\", None, None);\n    \
             connect(\"db\", p, None);\n}}\n"
        );
        assert_eq!(sugar(&source).unwrap(), source);
        let file = syn::parse_file(&source).unwrap();
        assert_eq!(ParamDefaults.edits(&file, &source), []);
    }

    #[test]
    fn no_proper_subset_of_the_edits_survives() {
        let source = format!(
            "{CONNECT}fn demo() {{\n    connect(\"db\", None, None);\n    \
             connect(\"db\", Some(1), None);\n}}\n"
        );
        let file = syn::parse_file(&source).unwrap();
        let edits = ParamDefaults.edits(&file, &source);
        assert_eq!(edits.len(), 7, "{edits:?}");
        for subset in 1..(1_u32 << edits.len()) - 1 {
            let part = edits
                .iter()
                .enumerate()
                .filter(|(at, _)| subset & (1 << at) != 0)
                .map(|(_, edit)| edit.clone())
                .collect();
            assert_eq!(
                convert(&source, &[&Fixed(part)]).unwrap(),
                source,
                "subset {subset:b}"
            );
        }
    }
}
