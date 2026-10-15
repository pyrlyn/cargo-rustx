//! Where a statement or item may start, judged from the tokens before it.
//!
//! Separate because keyword-like sugar (`var`, `bound`) is an ordinary identifier everywhere
//! else, so each such pass must recognise the same boundaries: sequence start, `;`, a `{ .. }`
//! block, inner and outer attributes.

use proc_macro2::{Delimiter, TokenTree};

use super::punct::is_punct;

/// Whether a statement or item may start right after `before`: at the start of a sequence, after
/// `;`, after a `{ .. }` block or after an inner attribute `#![..]`.
///
/// Outer attributes are not boundaries here: they belong to what follows them, so callers decide
/// whether to skip them first ([`outer_attributes_start`]) or to accept them as they are.
pub(crate) fn at_statement_start(before: &[TokenTree]) -> bool {
    match before {
        [] => true,
        [.., TokenTree::Punct(p)] => p.as_char() == ';',
        [.., TokenTree::Group(g)] if g.delimiter() == Delimiter::Brace => true,
        [.., hash, bang, TokenTree::Group(g)] if g.delimiter() == Delimiter::Bracket => {
            is_punct(hash, '#') && is_punct(bang, '!')
        }
        _ => false,
    }
}

/// Index of the first of the outer attributes (`#[..]`) right before `end`, or `end` if none.
pub(crate) fn outer_attributes_start(seq: &[TokenTree], mut end: usize) -> usize {
    while let [.., hash, TokenTree::Group(g)] = &seq[..end]
        && g.delimiter() == Delimiter::Bracket
        && is_punct(hash, '#')
    {
        end -= 2;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::tests::trees;

    #[test]
    fn sequence_start_semicolon_block_and_inner_attribute_start_a_statement() {
        for before in ["", "a;", "if c {}", "#![allow(unused)]"] {
            assert!(at_statement_start(&trees(before)), "{before}");
        }
    }

    #[test]
    fn expressions_and_outer_attributes_do_not_start_a_statement() {
        for before in ["let a =", "f(x) +", "x [0]", "#[allow(unused)]", "a ,"] {
            assert!(!at_statement_start(&trees(before)), "{before}");
        }
    }

    #[test]
    fn outer_attributes_are_skipped_back_to_the_first() {
        let seq = trees("a; #[x] #[y] b");
        assert_eq!(outer_attributes_start(&seq, 6), 2);
        assert_eq!(outer_attributes_start(&seq, 2), 2);
    }

    #[test]
    fn an_inner_attribute_is_not_an_outer_one() {
        let seq = trees("#![x] b");
        assert_eq!(outer_attributes_start(&seq, 3), 3);
    }
}
