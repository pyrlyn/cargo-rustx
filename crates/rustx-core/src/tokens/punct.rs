//! Punctuation checks and respacing on token trees.
//!
//! Separate because every pass asks the same questions of `Punct` tokens: which character, is it
//! glued to the next one (`Spacing::Joint`), is a `>` the head of `->`, is a `:` half of `::`.

use proc_macro2::{Punct, Spacing, TokenStream, TokenTree};

/// Whether `tree` is the punctuation `ch`, whatever its spacing.
pub(crate) fn is_punct(tree: &TokenTree, ch: char) -> bool {
    matches!(tree, TokenTree::Punct(p) if p.as_char() == ch)
}

/// Whether `tree` is the punctuation `ch` glued to the token after it, as the `-` of `->`.
pub(crate) fn is_joint(tree: &TokenTree, ch: char) -> bool {
    matches!(tree, TokenTree::Punct(p) if p.as_char() == ch && p.spacing() == Spacing::Joint)
}

/// Whether `tree` is the punctuation `ch` on its own, so `=` is not part of `==` or `=>`.
pub(crate) fn is_alone(tree: &TokenTree, ch: char) -> bool {
    matches!(tree, TokenTree::Punct(p) if p.as_char() == ch && p.spacing() == Spacing::Alone)
}

/// Whether the token at `gt` is the `>` of `->`.
pub(crate) fn is_arrow(seq: &[TokenTree], gt: usize) -> bool {
    is_punct(&seq[gt], '>') && gt.checked_sub(1).is_some_and(|m| is_joint(&seq[m], '-'))
}

/// Whether the token at `i` directly follows `->`.
pub(crate) fn follows_arrow(seq: &[TokenTree], i: usize) -> bool {
    i.checked_sub(1).is_some_and(|gt| is_arrow(seq, gt))
}

/// Whether the token at `at` is a `:` that is not half of `::`.
pub(crate) fn is_single_colon(seq: &[TokenTree], at: usize) -> bool {
    is_punct(&seq[at], ':')
        && !(is_joint(&seq[at], ':') && seq.get(at + 1).is_some_and(|next| is_punct(next, ':')))
        && !at
            .checked_sub(1)
            .is_some_and(|before| is_joint(&seq[before], ':'))
}

/// A copy of `punct` with `Spacing::Alone`, keeping its span.
pub(crate) fn alone(punct: &Punct) -> Punct {
    let mut fresh = Punct::new(punct.as_char(), Spacing::Alone);
    fresh.set_span(punct.span());
    fresh
}

/// Copies `tokens`, making the last one alone so it cannot glue onto the `>` that follows it.
pub(crate) fn loosen_end(tokens: impl IntoIterator<Item = TokenTree>) -> TokenStream {
    let mut trees: Vec<TokenTree> = tokens.into_iter().collect();
    if let Some(TokenTree::Punct(last)) = trees.last_mut() {
        *last = alone(last);
    }
    trees.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::tests::trees;

    #[test]
    fn punct_checks_tell_spacing_apart() {
        let seq = trees("= == a");
        assert!(is_punct(&seq[0], '=') && is_alone(&seq[0], '=') && !is_joint(&seq[0], '='));
        assert!(is_punct(&seq[1], '=') && is_joint(&seq[1], '=') && !is_alone(&seq[1], '='));
        assert!(!is_punct(&seq[3], '=') && !is_joint(&seq[3], '=') && !is_alone(&seq[3], '='));
    }

    #[test]
    fn only_the_gt_of_an_arrow_is_an_arrow() {
        let seq = trees("-> - > > x");
        assert!(is_arrow(&seq, 1));
        assert!(!is_arrow(&seq, 3) && !is_arrow(&seq, 4));
        assert!(follows_arrow(&seq, 2));
        assert!(!follows_arrow(&seq, 0) && !follows_arrow(&seq, 5));
    }

    #[test]
    fn a_single_colon_is_not_half_of_a_path_separator() {
        let seq = trees("a: b::c :");
        assert!(is_single_colon(&seq, 1));
        assert!(!is_single_colon(&seq, 3) && !is_single_colon(&seq, 4));
        assert!(is_single_colon(&seq, 6));
        assert!(!is_single_colon(&seq, 0));
    }

    #[test]
    fn loosen_end_unglues_only_the_last_punct() {
        // `+` of `+=` is joint; cut before `=` it ends the copy.
        let seq = trees("a + b +=");
        let out: Vec<TokenTree> = loosen_end(seq[..4].to_vec()).into_iter().collect();
        assert!(is_alone(&out[1], '+') && is_alone(&out[3], '+'));
        let unchanged: Vec<TokenTree> = loosen_end(trees("a ::b")).into_iter().collect();
        assert!(is_joint(&unchanged[1], ':'));
    }
}
