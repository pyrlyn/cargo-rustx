//! The arguments of one `#[delegate(..)]` attribute.

use proc_macro2::Span;
use syn::parse::{Parse, ParseStream};
use syn::spanned::Spanned;
use syn::{Attribute, Path, Token, TraitItem, braced, token};

/// One trait to delegate, as written in the attribute.
pub(super) struct Delegation {
    pub(super) path: Path,
    /// `Trait { fn m(&self); }`: the items to forward, for a trait the pass does not know.
    pub(super) listed: Option<Vec<TraitItem>>,
}

impl Delegation {
    /// The span generated tokens take: the trait's last segment, so errors rustc finds in the
    /// generated impl point at the trait the user named.
    pub(super) fn span(&self) -> Span {
        self.path
            .segments
            .last()
            .map_or_else(|| self.path.span(), |segment| segment.ident.span())
    }
}

/// Parses `Trait, Trait { items }`.
pub(super) fn parse(attr: &Attribute) -> syn::Result<Vec<Delegation>> {
    let all = attr.parse_args_with(|input: ParseStream| {
        let mut all: Vec<Delegation> = Vec::new();
        while !input.is_empty() {
            all.push(Delegation {
                path: input.parse()?,
                listed: listed(input)?,
            });
            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }
        Ok(all)
    })?;
    if all.is_empty() {
        return Err(syn::Error::new_spanned(
            attr,
            "`#[delegate(..)]` needs at least one trait, as in `#[delegate(Read)]`",
        ));
    }
    Ok(all)
}

fn listed(input: ParseStream) -> syn::Result<Option<Vec<TraitItem>>> {
    if !input.peek(token::Brace) {
        return Ok(None);
    }
    let content;
    braced!(content in input);
    trait_items(&content).map(Some)
}

/// Trait items up to the end of `input`, as in a trait body.
pub(super) fn trait_items(input: ParseStream) -> syn::Result<Vec<TraitItem>> {
    let mut items = Vec::new();
    while !input.is_empty() {
        items.push(TraitItem::parse(input)?);
    }
    Ok(items)
}
