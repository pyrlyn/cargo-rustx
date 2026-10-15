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
        errors: None,
    };
    walk.visit_file_mut(file);
    if !walk.fns.has_defaults() {
        return Ok(());
    }
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
    errors: Option<syn::Error>,
}

impl Walk {
    fn error(&mut self, span: Span, message: String) {
        combine(&mut self.errors, syn::Error::new(span, message));
    }

    fn call(&mut self, call: &mut ExprCall) {
        visit_mut::visit_expr_call_mut(self, call);
        let Expr::Path(ExprPath {
            qself: None, path, ..
        }) = &*call.func
        else {
            return;
        };
        let mut segments = path.segments.iter().rev();
        let Some(name) = segments.next().map(|segment| segment.ident.clone()) else {
            return;
        };
        let qualifier = match segments.next().map(|segment| segment.ident.to_string()) {
            Some(word) if word == "Self" => match &self.owner {
                Some(owner) => Some(owner.clone()),
                None => return,
            },
            other => other,
        };
        let shapes = self.fns.path_call(&name.to_string(), qualifier.as_deref());
        let plans = plans(&shapes, call.args.len());
        self.apply(&name, plans, &mut call.args, call.paren_token.span.close());
    }

    fn method_call(&mut self, call: &mut ExprMethodCall) {
        visit_mut::visit_expr_method_call_mut(self, call);
        let shapes = self.fns.method_call(&call.method.to_string());
        let plans = plans(&shapes, call.args.len());
        self.apply(
            &call.method,
            plans,
            &mut call.args,
            call.paren_token.span.close(),
        );
    }

    fn apply(&mut self, name: &Ident, plans: Option<Vec<Plan>>, args: &mut Args, close: Span) {
        let Some(plans) = plans else {
            return;
        };
        if plans.windows(2).any(|pair| pair[0] != pair[1]) {
            self.error(
                name.span(),
                format!(
                    "`{name}` matches several fns in this input that read this call \
                     differently; calls are matched by name only, so rename one of them"
                ),
            );
            return;
        }
        match plans.into_iter().next() {
            Some(Ok(slots)) => {
                *args = fill(mem::take(args), &slots, close);
                self.changed = true;
            }
            Some(Err(problems)) => {
                for problem in problems {
                    match problem {
                        Problem::TooMany { max } => {
                            let span = args.iter().nth(max).map_or(close, Spanned::span);
                            self.error(
                                span,
                                format!("too many arguments: `{name}` takes at most {max}"),
                            );
                        }
                        Problem::Missing { label } => self.error(
                            close,
                            format!(
                                "missing argument `{label}` of `{name}`; only parameters with \
                                 a default can be left out"
                            ),
                        ),
                    }
                }
            }
            None => {}
        }
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

/// The plans of every candidate, or `None` when none has a default and the call is plain Rust.
fn plans(shapes: &[&Shape], given: usize) -> Option<Vec<Plan>> {
    shapes
        .iter()
        .any(|shape| shape.has_default())
        .then(|| shapes.iter().map(|shape| plan(shape, given)).collect())
}

fn fill(args: Args, slots: &[Slot], close: Span) -> Args {
    let mut given: Vec<Option<Expr>> = args.into_iter().map(Some).collect();
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
        let Ok(body) = mac.parse_body::<MacroArgs>() else {
            return;
        };
        self.opaque(&mut mac.tokens, body, |walk, body| match body {
            MacroArgs::List(list) => list.iter_mut().for_each(|arg| walk.visit_expr_mut(arg)),
            MacroArgs::Repeat(value, _, count) => {
                walk.visit_expr_mut(value);
                walk.visit_expr_mut(count);
            }
        });
    }

    /// A parameter default is code too: it is lowered into the fn body after this pass.
    fn visit_attribute_mut(&mut self, attr: &mut Attribute) {
        if self.collecting {
            return;
        }
        let Some(Ok(expr)) = default_of(slice::from_ref(attr)).map(|default| default.expr) else {
            return;
        };
        if let Meta::List(list) = &mut attr.meta {
            self.opaque(&mut list.tokens, expr, |walk, expr| {
                walk.visit_expr_mut(expr);
            });
        }
    }
}
