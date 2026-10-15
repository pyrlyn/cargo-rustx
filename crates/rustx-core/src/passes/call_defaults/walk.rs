//! The walk over the file: signatures first, then every call that resolves to a fn with
//! defaults, in fn bodies, in parameter defaults and in macro bodies that parse as expressions.

use std::{mem, slice};

use proc_macro2::{Ident, Span, TokenStream};
use quote::ToTokens;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::token::{Paren, PathSep};
use syn::visit_mut::{self, VisitMut};
use syn::{
    Attribute, Expr, ExprCall, ExprMethodCall, ExprPath, ImplItemFn, ItemFn, ItemImpl, ItemTrait,
    Macro, Meta, Path, PathSegment, Token, TraitItemFn,
};

use super::collect::{Fns, Shape, type_name};
use super::named::{self, Named};
use super::plan::{Plan, Problem, Slot, plan};
use crate::passes::param_defaults::default_of;
use crate::tokens::syntax::combine;

type Args = Punctuated<Expr, Token![,]>;

pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    let mut walk = Walk {
        fns: Fns::default(),
        collecting: true,
        owner: None,
        changed: false,
        in_macro: 0,
        errors: None,
    };
    walk.visit_file_mut(file);
    // Even without defaults the second walk runs: named arguments still need a report.
    walk.collecting = false;
    walk.visit_file_mut(file);
    walk.errors.map_or(Ok(()), Err)
}

struct Walk {
    fns: Fns,
    /// The first walk only records signatures: a call may come before the fn it calls.
    collecting: bool,
    /// The type of the enclosing `impl` or trait, for `Self::f(..)` and for recording methods.
    owner: Option<String>,
    /// Whether a call was rewritten since the enclosing token stream was parsed, so a macro body
    /// or default without sugar keeps its tokens exactly.
    changed: bool,
    /// How many macro bodies enclose the walk: there a named argument of a call that is not
    /// sugar is the macro's own syntax, restored instead of reported.
    in_macro: usize,
    errors: Option<syn::Error>,
}

/// One argument of a call: its value, and its name when it was written `name: value`.
struct Arg {
    named: Option<Named>,
    value: Expr,
}

impl Walk {
    fn error(&mut self, span: Span, message: String) {
        combine(&mut self.errors, syn::Error::new(span, message));
    }

    fn call(&mut self, call: &mut ExprCall) {
        self.visit_expr_mut(&mut call.func);
        let Some(args) = self.arguments(&mut call.args) else {
            return;
        };
        let names = names(&args);
        let resolved = self.path_target(&call.func).and_then(|(name, qualifier)| {
            let shapes = self.fns.path_call(&name.to_string(), qualifier.as_deref());
            plans(&shapes, &names).map(|plans| (name, plans))
        });
        call.args = self.apply(resolved, args, call.paren_token.span.close());
    }

    fn method_call(&mut self, call: &mut ExprMethodCall) {
        self.visit_expr_mut(&mut call.receiver);
        let Some(args) = self.arguments(&mut call.args) else {
            return;
        };
        let shapes = self.fns.method_call(&call.method.to_string());
        let resolved = plans(&shapes, &names(&args)).map(|plans| (call.method.clone(), plans));
        call.args = self.apply(resolved, args, call.paren_token.span.close());
    }

    /// The name a path call is resolved by, and its qualifier with `Self` replaced by the
    /// enclosing type; `None` for anything else that is called.
    fn path_target(&self, func: &Expr) -> Option<(Ident, Option<String>)> {
        let Expr::Path(ExprPath {
            qself: None, path, ..
        }) = func
        else {
            return None;
        };
        let mut segments = path.segments.iter().rev();
        let name = segments.next()?.ident.clone();
        let qualifier = match segments.next().map(|segment| segment.ident.to_string()) {
            Some(word) if word == "Self" => Some(self.owner.clone()?),
            other => other,
        };
        Some((name, qualifier))
    }

    /// Takes a call's arguments apart, then visits each value; `None` after reporting a named
    /// argument that does not parse.
    fn arguments(&mut self, args: &mut Args) -> Option<Vec<Arg>> {
        let mut out = Vec::with_capacity(args.len());
        for value in mem::take(args) {
            out.push(match named::parse(&value) {
                None => Arg { named: None, value },
                Some(Ok((named, value))) => Arg {
                    named: Some(named),
                    value,
                },
                Some(Err(err)) => {
                    combine(&mut self.errors, err);
                    return None;
                }
            });
        }
        for arg in &mut out {
            self.visit_expr_mut(&mut arg.value);
        }
        Some(out)
    }

    /// The arguments of a call that resolved to the fns of `plans`, or put back as they were
    /// when it did not or cannot be rewritten.
    fn apply(&mut self, resolved: Option<(Ident, Vec<Plan>)>, args: Vec<Arg>, close: Span) -> Args {
        let Some((name, plans)) = resolved else {
            if self.in_macro == 0 {
                for named in args.iter().filter_map(|arg| arg.named.as_ref()) {
                    let message = format!(
                        "named argument `{}` needs a call to a fn with parameter defaults \
                         declared in this `sugar!` input",
                        named.name
                    );
                    self.error(named.name.span(), message);
                }
            }
            return unchanged(args);
        };
        if plans.windows(2).any(|pair| pair[0] != pair[1]) {
            self.error(
                name.span(),
                format!(
                    "`{name}` matches several fns in this input that read this call \
                     differently; calls are matched by name only, so rename one of them"
                ),
            );
            return unchanged(args);
        }
        match plans.into_iter().next() {
            Some(Ok(slots)) => {
                self.changed = true;
                fill(args, &slots, close)
            }
            Some(Err(problems)) => {
                for problem in problems {
                    self.report(&name, problem, &args, close);
                }
                unchanged(args)
            }
            None => unchanged(args),
        }
    }

    fn report(&mut self, name: &Ident, problem: Problem, args: &[Arg], close: Span) {
        let value = |at: usize| args.get(at).map_or(close, |arg| arg.value.span());
        let named = |at: usize| {
            args.get(at)
                .and_then(|arg| arg.named.as_ref())
                .map_or(close, |named| named.name.span())
        };
        let label = |at: usize| {
            args.get(at)
                .and_then(|arg| arg.named.as_ref())
                .map(|named| named.name.to_string())
                .unwrap_or_default()
        };
        let (span, message) = match problem {
            Problem::TooMany { max } => (
                value(max),
                format!("too many arguments: `{name}` takes at most {max}"),
            ),
            Problem::Missing { label } => (
                close,
                format!(
                    "missing argument `{label}` of `{name}`; only parameters with a default \
                     can be left out"
                ),
            ),
            Problem::Unknown { arg } => (
                named(arg),
                format!("`{name}` has no parameter named `{}`", label(arg)),
            ),
            Problem::NotDefaulted { arg } => (
                named(arg),
                format!(
                    "`{}` has no default, so it is passed by position, not by name",
                    label(arg)
                ),
            ),
            Problem::Duplicate { arg } => (
                named(arg),
                format!("`{}` is given more than once", label(arg)),
            ),
            Problem::PositionalAfterNamed { arg } => (
                value(arg),
                "a positional argument cannot follow a named one".to_owned(),
            ),
        };
        self.error(span, message);
    }

    /// Visits code that lives in an opaque token stream (a macro body, a parameter default) and
    /// re-emits it only when a call in it was rewritten.
    fn opaque<T: ToTokens>(
        &mut self,
        tokens: &mut TokenStream,
        mut parsed: T,
        visit: impl FnOnce(&mut Self, &mut T),
    ) {
        let outer = mem::replace(&mut self.changed, false);
        visit(self, &mut parsed);
        if self.changed {
            *tokens = parsed.into_token_stream();
        }
        self.changed |= outer;
    }

    fn enter_owner(&mut self, owner: String, visit: impl FnOnce(&mut Self)) {
        let outer = self.owner.replace(owner);
        visit(self);
        self.owner = outer;
    }

    fn record(&mut self, sig: &syn::Signature, associated: bool) {
        if self.collecting {
            let owner = associated.then(|| self.owner.clone().unwrap_or_default());
            self.fns.add(sig, owner);
        }
    }
}

fn names(args: &[Arg]) -> Vec<Option<String>> {
    args.iter()
        .map(|arg| arg.named.as_ref().map(|named| named.name.to_string()))
        .collect()
}

/// The plans of every candidate, or `None` when none has a default and the call is plain Rust.
fn plans(shapes: &[&Shape], names: &[Option<String>]) -> Option<Vec<Plan>> {
    shapes
        .iter()
        .any(|shape| shape.has_default())
        .then(|| shapes.iter().map(|shape| plan(shape, names)).collect())
}

fn unchanged(args: Vec<Arg>) -> Args {
    args.into_iter()
        .map(|arg| match arg.named {
            Some(named) => named::rebuild(named, &arg.value),
            None => arg.value,
        })
        .collect()
}

fn fill(args: Vec<Arg>, slots: &[Slot], close: Span) -> Args {
    let mut given: Vec<Option<Expr>> = args.into_iter().map(|arg| Some(arg.value)).collect();
    let mut take = |at: usize| given.get_mut(at).and_then(Option::take);
    slots
        .iter()
        .filter_map(|slot| match *slot {
            Slot::Plain(at) => take(at),
            Slot::Some(at) => take(at).map(|arg| option(arg.span(), "Some", Some(arg))),
            Slot::None => Some(option(close, "None", None)),
        })
        .collect()
}

/// `::core::option::Option::<variant>`, applied to `arg` when there is one.
fn option(span: Span, variant: &str, arg: Option<Expr>) -> Expr {
    let path = Path {
        leading_colon: Some(PathSep { spans: [span; 2] }),
        segments: ["core", "option", "Option", variant]
            .into_iter()
            .map(|segment| PathSegment::from(Ident::new(segment, span)))
            .collect(),
    };
    let func = Expr::Path(ExprPath {
        attrs: Vec::new(),
        qself: None,
        path,
    });
    match arg {
        None => func,
        Some(arg) => Expr::Call(ExprCall {
            attrs: Vec::new(),
            func: Box::new(func),
            paren_token: Paren(span),
            args: Punctuated::from_iter([arg]),
        }),
    }
}

/// A macro body made of expressions: `a, b, c` (`format!`, `assert_eq!`) or `value; count`
/// (`vec![x; n]`).
enum MacroArgs {
    List(Args),
    Repeat(Box<Expr>, Token![;], Box<Expr>),
}

impl Parse for MacroArgs {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let first: Expr = input.parse()?;
        if input.peek(Token![;]) {
            return Ok(Self::Repeat(
                Box::new(first),
                input.parse()?,
                input.parse()?,
            ));
        }
        let mut list = Args::new();
        list.push_value(first);
        while !input.is_empty() {
            list.push_punct(input.parse()?);
            if input.is_empty() {
                break;
            }
            list.push_value(input.parse()?);
        }
        Ok(Self::List(list))
    }
}

impl ToTokens for MacroArgs {
    fn to_tokens(&self, tokens: &mut TokenStream) {
        match self {
            Self::List(list) => list.to_tokens(tokens),
            Self::Repeat(value, semi, count) => {
                value.to_tokens(tokens);
                semi.to_tokens(tokens);
                count.to_tokens(tokens);
            }
        }
    }
}

impl VisitMut for Walk {
    fn visit_item_impl_mut(&mut self, item: &mut ItemImpl) {
        self.enter_owner(type_name(&item.self_ty), |walk| {
            visit_mut::visit_item_impl_mut(walk, item);
        });
    }

    fn visit_item_trait_mut(&mut self, item: &mut ItemTrait) {
        self.enter_owner(item.ident.to_string(), |walk| {
            visit_mut::visit_item_trait_mut(walk, item);
        });
    }

    fn visit_item_fn_mut(&mut self, item: &mut ItemFn) {
        self.record(&item.sig, false);
        visit_mut::visit_item_fn_mut(self, item);
    }

    fn visit_impl_item_fn_mut(&mut self, item: &mut ImplItemFn) {
        self.record(&item.sig, true);
        visit_mut::visit_impl_item_fn_mut(self, item);
    }

    fn visit_trait_item_fn_mut(&mut self, item: &mut TraitItemFn) {
        self.record(&item.sig, true);
        visit_mut::visit_trait_item_fn_mut(self, item);
    }

    fn visit_expr_mut(&mut self, expr: &mut Expr) {
        match expr {
            Expr::Call(call) if !self.collecting => self.call(call),
            Expr::MethodCall(call) if !self.collecting => self.method_call(call),
            _ => visit_mut::visit_expr_mut(self, expr),
        }
    }

    fn visit_macro_mut(&mut self, mac: &mut Macro) {
        if self.collecting {
            return;
        }
        if let Some(name) = named::marker_name(mac) {
            // A marker the walk reaches on its own is not an argument of any call.
            if self.in_macro == 0 {
                self.error(
                    name,
                    "a named argument such as `port: 6432` is only allowed in a call".to_owned(),
                );
            }
            return;
        }
        if let Ok(body) = mac.parse_body::<MacroArgs>() {
            self.in_macro += 1;
            self.opaque(&mut mac.tokens, body, |walk, body| match body {
                MacroArgs::List(list) => list.iter_mut().for_each(|arg| walk.visit_expr_mut(arg)),
                MacroArgs::Repeat(value, _, count) => {
                    walk.visit_expr_mut(value);
                    walk.visit_expr_mut(count);
                }
            });
            self.in_macro -= 1;
        }
        if let Some(restored) = named::restore(&mac.tokens) {
            mac.tokens = restored;
        }
    }

    /// A parameter default is code too: it is lowered into the fn body after this pass. Any
    /// other attribute belongs to whoever reads it, so it gets back what the user wrote.
    fn visit_attribute_mut(&mut self, attr: &mut Attribute) {
        if self.collecting {
            return;
        }
        let default = default_of(slice::from_ref(attr)).map(|default| default.expr);
        let Meta::List(list) = &mut attr.meta else {
            return;
        };
        match default {
            Some(Ok(expr)) => self.opaque(&mut list.tokens, expr, |walk, expr| {
                walk.visit_expr_mut(expr);
            }),
            // `param_defaults::lower` reports a default that does not parse.
            Some(Err(_)) => {}
            None => {
                if let Some(restored) = named::restore(&list.tokens) {
                    list.tokens = restored;
                }
            }
        }
    }
}
