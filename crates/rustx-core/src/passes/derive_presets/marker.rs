//! Token half: takes `derive Name = ..;` items out and records them as file-level markers.

use proc_macro2::{Ident, TokenStream, TokenTree};
use quote::quote_spanned;
use syn::Token;
use syn::parse::{ParseStream, Parser};

use super::{MARKER, Preset};
use crate::tokens::map_sequences_reporting;
use crate::tokens::position::{at_statement_start, outer_attributes_start, visibility_start};
use crate::tokens::punct::is_punct;
use crate::tokens::syntax::combine;

/// Derives of the standard library; a preset with one of these names would change what a plain
/// `#[derive(Debug)]` means.
const STD_DERIVES: [&str; 9] = [
    "Debug",
    "Clone",
    "Copy",
    "PartialEq",
    "Eq",
    "PartialOrd",
    "Ord",
    "Hash",
    "Default",
];

pub(super) fn rewrite(tokens: TokenStream) -> syn::Result<TokenStream> {
    let mut markers = Vec::new();
    let rest = map_sequences_reporting(tokens, |seq, errors| {
        rewrite_sequence(seq, errors, &mut markers)
    })?;
    // Inner attributes are only valid before the first item, so the markers all go to the front
    // whatever module their declaration was in.
    let mut out: TokenStream = markers.into_iter().collect();
    out.extend(rest);
    Ok(out)
}

fn rewrite_sequence(
    seq: &[TokenTree],
    errors: &mut Option<syn::Error>,
    markers: &mut Vec<TokenStream>,
) -> Vec<TokenTree> {
    let mut out = Vec::with_capacity(seq.len());
    let mut i = 0;
    while i < seq.len() {
        if let Some(start) = preset_at(seq, i) {
            match read(seq, start, i) {
                Ok((end, marker)) => {
                    // Attributes and visibility were already copied while walking up to `i`.
                    out.truncate(out.len() - (i - start));
                    markers.push(marker);
                    i = end;
                    continue;
                }
                Err(err) => combine(errors, err),
            }
        }
        out.push(seq[i].clone());
        i += 1;
    }
    out
}

/// If `seq[i]` is a `derive` preset keyword, returns where its attributes start.
///
/// `derive` is an ordinary identifier in Rust, so it is sugar only at item position and only
/// when an identifier other than `as` follows it, which valid Rust never has there.
fn preset_at(seq: &[TokenTree], i: usize) -> Option<usize> {
    let TokenTree::Ident(keyword) = &seq[i] else {
        return None;
    };
    if keyword != "derive" {
        return None;
    }
    match seq.get(i + 1)? {
        TokenTree::Ident(name) if name != "as" => {}
        _ => return None,
    }
    let start = outer_attributes_start(seq, visibility_start(seq, i));
    at_statement_start(&seq[..start]).then_some(start)
}

/// Reads the preset whose `derive` keyword is at `i`: the index after its `;` and its marker.
///
/// Every problem of one preset is reported together, so fixing one does not uncover the next.
fn read(seq: &[TokenTree], start: usize, i: usize) -> syn::Result<(usize, TokenStream)> {
    let keyword = seq[i].span();
    let mut errors: Option<syn::Error> = None;

    let vis = visibility_start(seq, i);
    if vis < i {
        combine(
            &mut errors,
            syn::Error::new(
                seq[vis].span(),
                "`derive` presets take no visibility: they exist only inside this `sugar!` input",
            ),
        );
    }
    for attr in seq[start..vis].chunks(2) {
        if !is_doc(attr) {
            combine(
                &mut errors,
                syn::Error::new(
                    attr[0].span(),
                    "`derive` presets take no attributes other than docs",
                ),
            );
        }
    }

    let semi = seq[i..]
        .iter()
        .position(|tree| is_punct(tree, ';'))
        .map(|at| i + at);
    let Some(semi) = semi else {
        // The list ran to the end of the sequence; its last token is the closest user token.
        let err = syn::Error::new(
            seq[seq.len() - 1].span(),
            "expected `;` after the derive list",
        );
        return Err(match errors {
            Some(mut all) => {
                all.combine(err);
                all
            }
            None => err,
        });
    };

    // The `;` is parsed too, so a missing `=` or a stray token is reported on a user token
    // instead of at an end of input that has no span of its own.
    let parse = |input: ParseStream| {
        let preset = input.parse::<Preset>()?;
        input
            .parse::<Token![;]>()
            .map_err(|_| input.error("expected `;` after the derive list"))?;
        Ok(preset)
    };
    match parse.parse2(seq[i + 1..=semi].iter().cloned().collect()) {
        Ok(preset) => {
            if let Some(std) = STD_DERIVES.iter().find(|std| preset.name == **std) {
                combine(
                    &mut errors,
                    syn::Error::new(
                        preset.name.span(),
                        format!(
                            "`derive` preset `{std}` shadows the std derive `{std}`; pick another name"
                        ),
                    ),
                );
            }
        }
        Err(err) => combine(&mut errors, err),
    }
    if let Some(errors) = errors {
        return Err(errors);
    }

    let body: TokenStream = seq[i + 1..semi].iter().cloned().collect();
    let marker = Ident::new(MARKER, keyword);
    Ok((semi + 1, quote_spanned! {keyword=> #![#marker(#body)] }))
}

/// Whether `attr` (`#` and its bracket group) is a doc comment or `#[doc = ..]`.
fn is_doc(attr: &[TokenTree]) -> bool {
    matches!(attr, [_, TokenTree::Group(g)]
        if matches!(g.stream().into_iter().next(), Some(TokenTree::Ident(i)) if i == "doc"))
}
