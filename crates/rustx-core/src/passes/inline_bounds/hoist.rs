//! AST half of inline bounds: markers and type-path bounds of a signature, and the bounds an
//! `impl` header or signature borrows from a struct of the input, become generics and `where`.
//!
//! The rules for telling a new generic from an existing name are in the parent module.

use std::collections::{HashMap, HashSet};
use std::mem;

use proc_macro2::{Ident, Span};
use quote::ToTokens;
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::visit_mut::{self, VisitMut};
use syn::{
    Attribute, Block, Expr, FnArg, GenericArgument, GenericParam, Generics, Item, ItemImpl,
    ItemTrait, Path, PathArguments, PredicateType, ReturnType, Signature, Token, Type, TypeParam,
    TypeParamBound, TypePath, WhereClause, WherePredicate,
};

use super::marker::MARKER;
use crate::tokens::syntax::{bare_name, combine, take_attr};

type Bounds = Punctuated<TypeParamBound, Token![+]>;

/// Type names every input can use without declaring them: primitives and prelude types.
const BUILTIN: &[&str] = &[
    "bool", "char", "str", "u8", "u16", "u32", "u64", "u128", "usize", "i8", "i16", "i32", "i64",
    "i128", "isize", "f32", "f64", "Box", "Option", "Result", "String", "Vec",
];

const NOT_A_NAME: &str =
    "an inline bound needs a bare generic name such as `T`; bound other types in `where`";

pub(super) fn lower(file: &mut syn::File) -> syn::Result<()> {
    let mut known = Known::default();
    known.visit_file_mut(file);
    let mut lower = Lower {
        known,
        scope: Vec::new(),
        errors: None,
    };
    lower.visit_file_mut(file);
    lower.errors.map_or(Ok(()), Err)
}

/// What the input declares: type names, and the type parameters of its structs, enums and
/// unions (`None` for a const parameter, which lends no bounds).
#[derive(Default)]
pub(crate) struct Known {
    types: HashSet<String>,
    adts: HashMap<String, Vec<Option<(Ident, Bounds)>>>,
}

impl Known {
    /// What `file` declares, collected as [`lower`] does, for the reverse rule (T19.9).
    #[cfg(feature = "sugar")]
    pub(crate) fn of(file: &syn::File) -> Self {
        let mut known = Self::default();
        known.visit_file_mut(&mut file.clone());
        known
    }

    pub(crate) fn is_type(&self, name: &Ident) -> bool {
        let name = name.to_string();
        BUILTIN.contains(&name.as_str()) || self.types.contains(&name)
    }

    fn adt(&mut self, name: &Ident, generics: &Generics) {
        self.types.insert(name.to_string());
        let params = generics
            .params
            .iter()
            .filter_map(|param| match param {
                GenericParam::Type(ty) => {
                    Some(Some((ty.ident.clone(), declared_bounds(ty, generics))))
                }
                GenericParam::Const(_) => Some(None),
                GenericParam::Lifetime(_) => None,
            })
            .collect();
        self.adts.entry(name.to_string()).or_insert(params);
    }
}

/// Bounds of `ty` from its declaration and from `where` predicates on the bare name.
fn declared_bounds(ty: &TypeParam, generics: &Generics) -> Bounds {
    let mut bounds = ty.bounds.clone();
    let predicates = generics
        .where_clause
        .iter()
        .flat_map(|clause| &clause.predicates);
    for predicate in predicates {
        if let WherePredicate::Type(predicate) = predicate
            && predicate.lifetimes.is_none()
            && bare_name(&predicate.bounded_ty) == Some(&ty.ident)
        {
            bounds.extend(predicate.bounds.iter().cloned());
        }
    }
    bounds
}

// Collecting through `VisitMut` because the crate enables only that visitor; nothing is changed.
impl VisitMut for Known {
    fn visit_item_mut(&mut self, item: &mut Item) {
        match item {
            Item::Struct(item) => self.adt(&item.ident, &item.generics),
            Item::Enum(item) => self.adt(&item.ident, &item.generics),
            Item::Union(item) => self.adt(&item.ident, &item.generics),
            Item::Type(syn::ItemType { ident, .. })
            | Item::Trait(ItemTrait { ident, .. })
            | Item::TraitAlias(syn::ItemTraitAlias { ident, .. }) => {
                self.types.insert(ident.to_string());
            }
            _ => {}
        }
        visit_mut::visit_item_mut(self, item);
    }

    fn visit_use_name_mut(&mut self, name: &mut syn::UseName) {
        self.types.insert(name.ident.to_string());
    }

    fn visit_use_rename_mut(&mut self, rename: &mut syn::UseRename) {
        self.types.insert(rename.rename.to_string());
    }
}

/// Walks the file; `scope` holds the generics of the enclosing `impl` or `trait`.
struct Lower {
    known: Known,
    scope: Vec<Ident>,
    errors: Option<syn::Error>,
}

impl Lower {
    fn absorb(&mut self, errors: Option<syn::Error>) {
        if let Some(err) = errors {
            combine(&mut self.errors, err);
        }
    }
}

impl VisitMut for Lower {
    fn visit_item_impl_mut(&mut self, item: &mut ItemImpl) {
        let mut header = Header::new(&self.known, param_names(&item.generics));
        header.visit_type_mut(&mut item.self_ty);
        let errors = header.finish(&mut item.generics);
        self.absorb(errors);
        let saved = mem::replace(&mut self.scope, param_names(&item.generics));
        visit_mut::visit_item_impl_mut(self, item);
        self.scope = saved;
    }

    fn visit_item_trait_mut(&mut self, item: &mut ItemTrait) {
        let saved = mem::replace(&mut self.scope, param_names(&item.generics));
        visit_mut::visit_item_trait_mut(self, item);
        self.scope = saved;
    }

    // Items in a body cannot use the generics of the item around it.
    fn visit_block_mut(&mut self, block: &mut Block) {
        let saved = mem::take(&mut self.scope);
        visit_mut::visit_block_mut(self, block);
        self.scope = saved;
    }

    fn visit_signature_mut(&mut self, sig: &mut Signature) {
        let mut declared = self.scope.clone();
        declared.extend(param_names(&sig.generics));
        let mut header = Header::new(&self.known, declared);
        for arg in &mut sig.inputs {
            match arg {
                FnArg::Typed(param) => {
                    if let Some((colon, bounds)) = header.take_marker(&mut param.attrs) {
                        match bare_name(&param.ty) {
                            Some(name) => header.bound(name, colon, bounds),
                            None => header.error(param.ty.span(), NOT_A_NAME),
                        }
                    }
                    header.visit_type_mut(&mut param.ty);
                }
                FnArg::Receiver(receiver) => {
                    if let Some((colon, _)) = header.take_marker(&mut receiver.attrs) {
                        header.error(
                            colon,
                            "`self` takes no inline bound; bound `Self` in `where`",
                        );
                    }
                }
            }
        }
        if let ReturnType::Type(_, ty) = &mut sig.output {
            header.visit_type_mut(ty);
        }
        let errors = header.finish(&mut sig.generics);
        self.absorb(errors);
        visit_mut::visit_signature_mut(self, sig);
    }
}

/// The generics one `impl` header or signature gains, collected while its types are walked.
struct Header<'k> {
    known: &'k Known,
    declared: Vec<Ident>,
    added: Vec<Ident>,
    predicates: Vec<(Ident, Span, Bounds)>,
    errors: Option<syn::Error>,
}

impl<'k> Header<'k> {
    fn new(known: &'k Known, declared: Vec<Ident>) -> Self {
        Header {
            known,
            declared,
            added: Vec::new(),
            predicates: Vec::new(),
            errors: None,
        }
    }

    fn error(&mut self, span: Span, message: impl std::fmt::Display) {
        combine(&mut self.errors, syn::Error::new(span, message));
    }

    /// Removes the marker the token half put on a parameter and returns its `:` and bounds.
    fn take_marker(&mut self, attrs: &mut Vec<Attribute>) -> Option<(Span, Bounds)> {
        let marker = take_attr(attrs, MARKER)?;
        match marker.parse_args_with(Bounds::parse_separated_nonempty) {
            Ok(bounds) => Some((marker.pound_token.spans[0], bounds)),
            Err(err) => {
                combine(&mut self.errors, err);
                None
            }
        }
    }

    fn in_scope(&self, name: &Ident) -> bool {
        name == "Self" || self.declared.contains(name)
    }

    /// An explicit inline bound: declares `name` unless it is in scope; a known type is an error.
    fn bound(&mut self, name: &Ident, colon: Span, bounds: Bounds) {
        if !self.in_scope(name) {
            if self.known.is_type(name) {
                let message =
                    format!("`{name}` names a type here, not a new generic; bound it in `where`");
                self.error(name.span(), message);
                return;
            }
            self.declare(name);
        }
        self.predicate(name, colon, bounds);
    }

    /// A struct bound borrowed for `name`: only when `name` is neither in scope nor a type.
    fn inherit(&mut self, name: &Ident, bounds: Bounds) {
        if self.in_scope(name) || self.known.is_type(name) {
            return;
        }
        self.declare(name);
        if !bounds.is_empty() {
            self.predicate(name, name.span(), bounds);
        }
    }

    fn declare(&mut self, name: &Ident) {
        self.declared.push(name.clone());
        self.added.push(name.clone());
    }

    fn predicate(&mut self, name: &Ident, colon: Span, bounds: Bounds) {
        if !self.predicates.iter().any(|(seen, ..)| seen == name) {
            self.predicates.push((name.clone(), colon, Bounds::new()));
        }
        let Some((.., merged)) = self.predicates.iter_mut().find(|(seen, ..)| seen == name) else {
            return;
        };
        for bound in bounds {
            let printed = bound.to_token_stream().to_string();
            if merged
                .iter()
                .all(|seen| seen.to_token_stream().to_string() != printed)
            {
                merged.push(bound);
            }
        }
    }

    /// `Result<T, E: Error>` in a type: the constraint becomes the argument `E` plus a bound.
    fn lift(&mut self, arg: &mut GenericArgument) {
        let GenericArgument::Constraint(constraint) = arg else {
            return;
        };
        if constraint.generics.is_some() {
            self.error(constraint.ident.span(), NOT_A_NAME);
            return;
        }
        let name = constraint.ident.clone();
        let colon = constraint.colon_token.spans[0];
        let bounds = mem::take(&mut constraint.bounds);
        *arg = GenericArgument::Type(name_type(&name));
        self.bound(&name, colon, bounds);
    }

    /// `Cache<K, V>` naming a struct of the input with its own parameter names borrows its bounds.
    fn inherit_adt(&mut self, path: &Path) {
        let known = self.known;
        let (None, 1, Some(segment)) = (
            &path.leading_colon,
            path.segments.len(),
            path.segments.first(),
        ) else {
            return;
        };
        let (PathArguments::AngleBracketed(args), Some(params)) = (
            &segment.arguments,
            known.adts.get(&segment.ident.to_string()),
        ) else {
            return;
        };
        let args = args
            .args
            .iter()
            .filter(|arg| !matches!(arg, GenericArgument::Lifetime(_)));
        let pairs: Vec<(Ident, Bounds, Type)> = params
            .iter()
            .zip(args)
            .filter_map(|(param, arg)| match (param, arg) {
                (Some((name, bounds)), GenericArgument::Type(ty)) => {
                    Some((name.clone(), bounds.clone(), ty.clone()))
                }
                _ => None,
            })
            .collect();
        let mut substitute = Substitute(pairs.iter().map(|(name, _, ty)| (name, ty)).collect());
        for (param, bounds, arg) in &pairs {
            let Some(name) = bare_name(arg).filter(|name| *name == param) else {
                continue;
            };
            let mut bounds = bounds.clone();
            for bound in &mut bounds {
                substitute.visit_type_param_bound_mut(bound);
            }
            self.inherit(name, bounds);
        }
    }

    /// Adds the collected generics and predicates; returns the errors met on the way.
    fn finish(self, generics: &mut Generics) -> Option<syn::Error> {
        for name in self.added {
            let span = name.span();
            generics.lt_token.get_or_insert(Token![<](span));
            generics.gt_token.get_or_insert(Token![>](span));
            generics
                .params
                .push(GenericParam::Type(TypeParam::from(name)));
        }
        if let Some((first, ..)) = self.predicates.first() {
            let span = first.span();
            let clause = generics.where_clause.get_or_insert_with(|| WhereClause {
                where_token: Token![where](span),
                predicates: Punctuated::new(),
            });
            for (name, colon, bounds) in self.predicates {
                clause.predicates.push(WherePredicate::Type(PredicateType {
                    attrs: Vec::new(),
                    lifetimes: None,
                    bounded_ty: name_type(&name),
                    colon_token: Token![:](colon),
                    bounds,
                }));
            }
        }
        self.errors
    }
}

impl VisitMut for Header<'_> {
    // Only type paths: the paths of `impl Tr<A: B>`, `dyn ..` and bounds are trait paths, where
    // a constraint is a valid associated-type bound.
    fn visit_type_path_mut(&mut self, ty: &mut TypePath) {
        if ty.qself.is_none() {
            for segment in &mut ty.path.segments {
                if let PathArguments::AngleBracketed(args) = &mut segment.arguments {
                    for arg in &mut args.args {
                        self.lift(arg);
                    }
                }
            }
            self.inherit_adt(&ty.path);
        }
        visit_mut::visit_type_path_mut(self, ty);
    }

    // Expressions (array lengths, const arguments) belong to no signature.
    fn visit_expr_mut(&mut self, _: &mut Expr) {}
}

/// Replaces a struct's parameter names in its bounds with the arguments of one use.
struct Substitute<'a>(Vec<(&'a Ident, &'a Type)>);

impl VisitMut for Substitute<'_> {
    fn visit_type_mut(&mut self, ty: &mut Type) {
        let arg = bare_name(ty).and_then(|name| self.0.iter().find(|(param, _)| *param == name));
        match arg {
            Some((_, arg)) => *ty = (*arg).clone(),
            None => visit_mut::visit_type_mut(self, ty),
        }
    }
}

fn param_names(generics: &Generics) -> Vec<Ident> {
    let types = generics.type_params().map(|ty| ty.ident.clone());
    types
        .chain(generics.const_params().map(|c| c.ident.clone()))
        .collect()
}

fn name_type(name: &Ident) -> Type {
    Type::Path(TypePath {
        attrs: Vec::new(),
        qself: None,
        path: Path::from(name.clone()),
    })
}
