//! The arguments of one `#[delegate(..)]` attribute.

use proc_macro2::{Ident, Span};
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::spanned::Spanned;
use syn::{Attribute, Path, Token, TraitItem, braced, parenthesized, token};

syn::custom_keyword!(except);

/// One trait to delegate, as written in the attribute.
pub(super) struct Delegation {
    pub(super) path: Path,
    /// `Trait { fn m(&self); }`: the items to forward, for a trait the pass does not know.
    pub(super) listed: Option<Vec<TraitItem>>,
    /// Methods the user implements by hand; empty when there is no `except(..)`.
    pub(super) except: Vec<Ident>,
    /// The `except` keyword, where an error about the whole list points.
    pub(super) except_span: Option<Span>,
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

/// Parses `Trait, Trait { items }, Trait, except(a, b)`; `except` belongs to the trait before it.
pub(super) fn parse(attr: &Attribute) -> syn::Result<Vec<Delegation>> {
    let all = attr.parse_args_with(|input: ParseStream| {
        let mut all: Vec<Delegation> = Vec::new();
        while !input.is_empty() {
            if input.peek(except) && input.peek2(token::Paren) {
                add_except(input, all.last_mut())?;
            } else {
                all.push(Delegation {
                    path: input.parse()?,
                    listed: listed(input)?,
                    except: Vec::new(),
                    except_span: None,
                });
            }
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

fn add_except(input: ParseStream, last: Option<&mut Delegation>) -> syn::Result<()> {
    let keyword: except = input.parse()?;
    let content;
    parenthesized!(content in input);
    let names = Punctuated::<Ident, Token![,]>::parse_terminated(&content)?;
    let Some(last) = last else {
        return Err(syn::Error::new(
            keyword.span,
            "`except(..)` must follow the trait it applies to, as in `Write, except(write)`",
        ));
    };
    if names.is_empty() {
        return Err(syn::Error::new(
            keyword.span,
            "`except(..)` needs the methods implemented by hand",
        ));
    }
    last.except.extend(names);
    last.except_span.get_or_insert(keyword.span);
    Ok(())
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
