//! The fns a call can resolve to: every signature of the input, by name.
//!
//! Fns without defaults are recorded too, so a call whose name also fits one of them is reported
//! as ambiguous instead of being filled in for the wrong fn. `widen` resolves calls the same way,
//! to find the parameter types its arguments go into.

use std::collections::HashMap;

use proc_macro2::Ident;
use quote::ToTokens;
use syn::{Expr, ExprPath, FnArg, Pat, Signature, Type};

use crate::passes::param_defaults::default_of;

/// The parameters a call has to supply, the receiver left out.
#[derive(Clone, PartialEq, Eq, Debug)]
pub(in crate::passes) struct Shape {
    pub(in crate::passes) params: Vec<Param>,
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(in crate::passes) struct Param {
    /// The binding, when the parameter is a plain name a call can refer to.
    pub(super) name: Option<String>,
    /// How errors refer to the parameter: its name or its pattern as written.
    pub(super) label: String,
    pub(super) default: bool,
    /// The type as written.
    pub(in crate::passes) ty: Type,
}

impl Shape {
    pub(super) fn has_default(&self) -> bool {
        self.params.iter().any(|param| param.default)
    }
}

/// Where a fn is declared, which decides how a path call can name it.
enum Owner {
    Free,
    /// An `impl` or a trait: the last path segment of its type, empty when it has none.
    Type(String),
}

struct Entry {
    owner: Owner,
    receiver: bool,
    shape: Shape,
}

#[derive(Default)]
pub(in crate::passes) struct Fns {
    by_name: HashMap<String, Vec<Entry>>,
}

impl Fns {
    /// Records a signature; `owner` is the name of the enclosing `impl` or trait type, `None`
    /// for a free fn.
    pub(in crate::passes) fn add(&mut self, sig: &Signature, owner: Option<String>) {
        let params = sig
            .inputs
            .iter()
            .filter_map(|arg| match arg {
                FnArg::Typed(param) => Some(Param {
                    name: plain_name(&param.pat),
                    label: plain_name(&param.pat)
                        .unwrap_or_else(|| param.pat.to_token_stream().to_string()),
                    default: default_of(&param.attrs).is_some(),
                    ty: (*param.ty).clone(),
                }),
                FnArg::Receiver(_) => None,
            })
            .collect();
        self.by_name
            .entry(sig.ident.to_string())
            .or_default()
            .push(Entry {
                owner: owner.map_or(Owner::Free, Owner::Type),
                receiver: sig.receiver().is_some(),
                shape: Shape { params },
            });
    }

    /// The shapes `qualifier::name(..)` can call: free fns (the qualifier may be a module) and
    /// associated fns of a type named `qualifier`; a bare `name(..)` only reaches free fns.
    pub(in crate::passes) fn path_call(&self, name: &str, qualifier: Option<&str>) -> Vec<&Shape> {
        self.matching(name, |entry| {
            !entry.receiver
                && match (&entry.owner, qualifier) {
                    (Owner::Free, _) => true,
                    (Owner::Type(owner), Some(qualifier)) => owner == qualifier,
                    (Owner::Type(_), None) => false,
                }
        })
    }

    /// The shapes `x.name(..)` can call: every method with a receiver, whatever its type.
    pub(super) fn method_call(&self, name: &str) -> Vec<&Shape> {
        self.matching(name, |entry| entry.receiver)
    }

    fn matching(&self, name: &str, keep: impl Fn(&Entry) -> bool) -> Vec<&Shape> {
        self.by_name
            .get(name)
            .into_iter()
            .flatten()
            .filter(|entry| keep(entry))
            .map(|entry| &entry.shape)
            .collect()
    }
}

/// The name a path call is resolved by, and its qualifier with `Self` replaced by `owner`, the
/// enclosing type; `None` for anything else that is called.
pub(in crate::passes) fn path_target(
    func: &Expr,
    owner: Option<&str>,
) -> Option<(Ident, Option<String>)> {
    let Expr::Path(ExprPath {
        qself: None, path, ..
    }) = func
    else {
        return None;
    };
    let mut segments = path.segments.iter().rev();
    let name = segments.next()?.ident.clone();
    let qualifier = match segments.next().map(|segment| segment.ident.to_string()) {
        Some(word) if word == "Self" => Some(owner?.to_owned()),
        other => other,
    };
    Some((name, qualifier))
}

/// The last path segment of an `impl`'s self type, empty when it is not a path.
pub(in crate::passes) fn type_name(ty: &Type) -> String {
    match ty {
        Type::Path(path) => path
            .path
            .segments
            .last()
            .map(|segment| segment.ident.to_string())
            .unwrap_or_default(),
        _ => String::new(),
    }
}

fn plain_name(pat: &Pat) -> Option<String> {
    match pat {
        Pat::Ident(ident) if ident.subpat.is_none() => Some(ident.ident.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use syn::ItemFn;

    fn fns(sources: &[(&str, Option<&str>)]) -> Fns {
        let mut fns = Fns::default();
        for (source, owner) in sources {
            let item: ItemFn = syn::parse_str(source).unwrap();
            fns.add(&item.sig, owner.map(str::to_owned));
        }
        fns
    }

    #[test]
    fn a_bare_call_reaches_free_fns_and_a_qualified_one_its_type_too() {
        let fns = fns(&[
            ("fn new(#[__rustx_default(1)] a: u8) {}", None),
            ("fn new(x: u8, y: u8) {}", Some("Point")),
            ("fn new(&self) {}", Some("Point")),
        ]);
        assert_eq!(fns.path_call("new", None).len(), 1);
        assert_eq!(fns.path_call("new", Some("Point")).len(), 2);
        assert_eq!(fns.path_call("new", Some("Vec")).len(), 1);
        assert_eq!(fns.method_call("new").len(), 1);
        assert_eq!(fns.method_call("other").len(), 0);
    }

    #[test]
    fn parameters_keep_their_written_types() {
        let fns = fns(&[("fn f(a: u8, b: Option<u16>) {}", None)]);
        let types: Vec<String> = fns.path_call("f", None)[0]
            .params
            .iter()
            .map(|param| param.ty.to_token_stream().to_string())
            .collect();
        assert_eq!(types, ["u8", "Option < u16 >"]);
    }

    #[test]
    fn a_path_call_is_named_by_its_last_segment_and_self_by_the_owner() {
        let target = |source: &str, owner: Option<&str>| {
            path_target(&syn::parse_str(source).unwrap(), owner)
                .map(|(name, qualifier)| (name.to_string(), qualifier))
        };
        assert_eq!(target("a::f", None), Some(("f".into(), Some("a".into()))));
        assert_eq!(
            target("Self::f", Some("P")),
            Some(("f".into(), Some("P".into())))
        );
        assert_eq!(target("Self::f", None), None);
        assert_eq!(target("(f)", None), None);
    }

    #[test]
    fn parameters_record_names_labels_and_defaults() {
        let fns = fns(&[(
            "fn f(&self, a: u8, (b, c): (u8, u8), #[__rustx_default(1)] d: u8) {}",
            None,
        )]);
        let shape = fns.method_call("f")[0];
        let seen: Vec<(Option<&str>, &str, bool)> = shape
            .params
            .iter()
            .map(|param| (param.name.as_deref(), param.label.as_str(), param.default))
            .collect();
        assert_eq!(
            seen,
            [
                (Some("a"), "a", false),
                (None, "(b , c)", false),
                (Some("d"), "d", true)
            ]
        );
    }

    #[test]
    fn an_impl_is_named_by_the_last_segment_of_its_type() {
        let name = |source: &str| type_name(&syn::parse_str(source).unwrap());
        assert_eq!(name("a::Point<T>"), "Point");
        assert_eq!(name("&Point"), "");
    }
}
