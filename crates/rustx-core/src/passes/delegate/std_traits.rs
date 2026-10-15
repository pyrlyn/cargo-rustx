//! The std traits `#[delegate]` knows without a method list.
//!
//! Only required items are listed. Provided methods are built on them, so the wrapper inherits
//! them and they go through the wrapper's own methods: with `except(write)`, `write_all` on a
//! counting writer still counts. Forwarding `Iterator::size_hint` or `Read::read_exact` would
//! skip a hand-written `next` or `read`, and would tie the table to std's growing list of
//! provided methods.

use proc_macro2::{Span, TokenStream};
use quote::quote_spanned;
use syn::parse::Parser;
use syn::{Path, TraitItem};

use super::spec::trait_items;

/// First segments of a path that may name a std trait: `Read`, `io::Read`, `std::io::Read`.
const STD_ROOTS: &[&str] = &["std", "core", "alloc", "io", "fmt", "iter", "hash"];

/// The fully qualified path and required items of the std trait `path` names, if it is one.
pub(super) fn lookup(path: &Path, span: Span) -> Option<syn::Result<(Path, Vec<TraitItem>)>> {
    let first = path.segments.first()?;
    let last = path.segments.last()?;
    let rooted = path.segments.len() == 1 && path.leading_colon.is_none()
        || STD_ROOTS.iter().any(|root| first.ident == root);
    if !rooted || !last.arguments.is_none() {
        return None;
    }
    let (qualified, items) = table(&last.ident.to_string(), span)?;
    Some(syn::parse2(qualified).and_then(|qualified| {
        let items = trait_items.parse2(items)?;
        Ok((qualified, items))
    }))
}

fn table(name: &str, span: Span) -> Option<(TokenStream, TokenStream)> {
    let io = quote_spanned!(span=> ::std::io::Result);
    let fmt = quote_spanned!(span=>
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result;
    );
    let entry = match name {
        "Read" => (
            quote_spanned!(span=> ::std::io::Read),
            quote_spanned!(span=> fn read(&mut self, buf: &mut [u8]) -> #io<usize>;),
        ),
        "Write" => (
            quote_spanned!(span=> ::std::io::Write),
            quote_spanned!(span=>
                fn write(&mut self, buf: &[u8]) -> #io<usize>;
                fn flush(&mut self) -> #io<()>;
            ),
        ),
        "BufRead" => (
            quote_spanned!(span=> ::std::io::BufRead),
            quote_spanned!(span=>
                fn fill_buf(&mut self) -> #io<&[u8]>;
                fn consume(&mut self, amount: usize);
            ),
        ),
        "Seek" => (
            quote_spanned!(span=> ::std::io::Seek),
            quote_spanned!(span=> fn seek(&mut self, pos: ::std::io::SeekFrom) -> #io<u64>;),
        ),
        "Iterator" => (
            quote_spanned!(span=> ::core::iter::Iterator),
            quote_spanned!(span=>
                type Item;
                fn next(&mut self) -> ::core::option::Option<Self::Item>;
            ),
        ),
        "DoubleEndedIterator" => (
            quote_spanned!(span=> ::core::iter::DoubleEndedIterator),
            quote_spanned!(span=>
                fn next_back(&mut self) -> ::core::option::Option<Self::Item>;
            ),
        ),
        "Display" => (quote_spanned!(span=> ::core::fmt::Display), fmt),
        "Debug" => (quote_spanned!(span=> ::core::fmt::Debug), fmt),
        "Hash" => (
            quote_spanned!(span=> ::core::hash::Hash),
            quote_spanned!(span=> fn hash<H: ::core::hash::Hasher>(&self, state: &mut H);),
        ),
        _ => return None,
    };
    Some(entry)
}
