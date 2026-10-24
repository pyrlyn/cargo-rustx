//! Angle-bracket depth and matching `>` in a flat token sequence.
//!
//! Separate because `<` and `>` are plain punctuation to a token tree, not a group, so every pass
//! that needs generics counts them itself; one counter keeps `>>` and `->` handled alike.

use proc_macro2::TokenTree;

use super::punct::{is_arrow, is_punct};

/// Angle-bracket depth after the token at `at`, given the depth before it.
///
/// Each `>` of a `>>` closes one level; the `>` of `->` closes none.
#[must_use]
pub fn angle_depth(seq: &[TokenTree], at: usize, depth: usize) -> usize {
    let Some(tree) = seq.get(at) else {
        return depth;
    };
    match tree {
        tree if is_punct(tree, '<') => depth + 1,
        tree if is_punct(tree, '>') && !is_arrow(seq, at) => depth.saturating_sub(1),
        _ => depth,
    }
}

/// Index of the `>` that closes the `<` at `open`, or `None` if the sequence ends first.
#[must_use]
pub fn matching_close(seq: &[TokenTree], open: usize) -> Option<usize> {
    if !seq.get(open).is_some_and(|tree| is_punct(tree, '<')) {
        return None;
    }
    let mut depth = 0;
    for at in open..seq.len() {
        if is_punct(&seq[at], ';') {
            return None;
        }
        depth = angle_depth(seq, at, depth);
        if depth == 0 {
            return Some(at);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tokens::tests::trees;

    #[test]
    fn each_gt_of_a_shift_closes_one_level() {
        // < A < B >> ( )
        let seq = trees("<A<B>>()");
        assert_eq!(matching_close(&seq, 0), Some(5));
        assert_eq!(matching_close(&seq, 2), Some(4));
    }

    #[test]
    fn the_gt_of_an_arrow_closes_nothing() {
        // < F ( ) - > u8 >
        let seq = trees("<F() -> u8>");
        assert_eq!(matching_close(&seq, 0), Some(6));
        assert_eq!(angle_depth(&seq, 4, 1), 1);
    }

    #[test]
    fn an_unclosed_angle_has_no_match() {
        assert_eq!(matching_close(&trees("<A<B>"), 0), None);
    }

    #[test]
    fn depth_never_goes_below_zero() {
        assert_eq!(angle_depth(&trees(">"), 0, 0), 0);
    }
}
