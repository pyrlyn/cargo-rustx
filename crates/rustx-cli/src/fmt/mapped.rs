//! The source with every sugar spelling replaced by a placeholder, and the way back.
//!
//! A placeholder is `pre`, an id and `post`: valid Rust of the sugar's syntactic role, so rustfmt
//! formats the code around it as usual. The id is `<marker><n>`, with a marker the source never
//! has before a digit, so the placeholders are found again by text after rustfmt.

use std::ops::Range;

const MOVED: &str = "rustfmt moved the placeholder of this sugar";
const DROPPED: &str = "rustfmt dropped the placeholder of this sugar";

/// One sugar spelling and the placeholder that stands in for it.
pub(super) struct Placeholder {
    /// Source bytes replaced by the placeholder.
    range: Range<usize>,
    pre: &'static str,
    post: String,
    /// The text that comes back in place of the placeholder.
    sugar: String,
}

impl Placeholder {
    pub(super) fn new(source: &str, range: Range<usize>, pre: &'static str, post: &str) -> Self {
        let (sugar, post) = (normalize(&source[range.clone()]), post.to_owned());
        Self {
            range,
            pre,
            post,
            sugar,
        }
    }
}

/// Collapses whitespace, unless the text may hold a string literal or a comment, which keep it.
fn normalize(text: &str) -> String {
    if text.contains(['"', '/']) {
        return text.to_owned();
    }
    let words = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let trail = text.ends_with(char::is_whitespace) && !words.is_empty();
    words + if trail { " " } else { "" }
}

pub(super) struct Mapped {
    /// The source with placeholders, which rustfmt parses.
    pub(super) text: String,
    marker: String,
    placeholders: Vec<Placeholder>,
    /// Each replaced source range with the range of its replacement in `text`.
    edits: Vec<(Range<usize>, Range<usize>)>,
}

impl Mapped {
    /// Replaces each spelling in `found`; one inside another's text stays part of that text.
    pub(super) fn new(source: &str, mut found: Vec<Placeholder>) -> Self {
        let marker = (1..=source.len() + 1)
            .map(|n| format!("_{}", "R".repeat(n)))
            .find(|marker| !has_id(source, marker))
            .unwrap_or_default();
        found.sort_by_key(|p| p.range.start);
        let (mut text, mut edits, mut placeholders) = (String::new(), Vec::new(), Vec::new());
        let mut at = 0;
        for p in found {
            if p.range.start < at {
                continue;
            }
            text.push_str(&source[at..p.range.start]);
            let start = text.len();
            let id = placeholders.len();
            text += &[p.pre, &marker, &id.to_string(), &p.post].concat();
            edits.push((p.range.clone(), start..text.len()));
            at = p.range.end;
            placeholders.push(p);
        }
        text.push_str(&source[at..]);
        Self {
            text,
            marker,
            placeholders,
            edits,
        }
    }

    /// The source offset that `offset` in [`Self::text`] comes from.
    pub(super) fn source_offset(&self, offset: usize) -> usize {
        let mut source = offset;
        for (from, to) in self.edits.iter().take_while(|(_, to)| to.start <= offset) {
            source = if offset < to.end {
                from.start
            } else {
                from.end + (offset - to.end)
            };
        }
        source
    }

    /// Puts the sugar back into `formatted`, rustfmt's output for [`Self::text`].
    ///
    /// # Errors
    ///
    /// The source offset and a message for a placeholder rustfmt dropped, split or duplicated.
    pub(super) fn restore(&self, formatted: &str) -> Result<String, (usize, String)> {
        let mut seen = vec![false; self.placeholders.len()];
        let mut edits: Vec<(Range<usize>, &str)> = Vec::new();
        for (at, _) in formatted.match_indices(&self.marker) {
            let rest = &formatted[at + self.marker.len()..];
            let digits =
                &rest[..rest.len() - rest.trim_start_matches(|c: char| c.is_ascii_digit()).len()];
            let Some((id, p)) = digits
                .parse::<usize>()
                .ok()
                .and_then(|id| Some((id, self.placeholders.get(id)?)))
            else {
                continue;
            };
            let id_end = at + self.marker.len() + digits.len();
            let span = match_before(formatted, at, p.pre).zip(match_at(formatted, id_end, &p.post));
            let Some((start, end)) = span.filter(|_| !seen[id]) else {
                return Err((p.range.start, MOVED.into()));
            };
            edits.push((start..end, &p.sugar));
            seen[id] = true;
        }
        if let Some(lost) = seen.iter().position(|seen| !seen) {
            return Err((self.placeholders[lost].range.start, DROPPED.into()));
        }
        let mut out = formatted.to_owned();
        for (range, text) in edits.into_iter().rev() {
            out.replace_range(range, text);
        }
        Ok(out)
    }
}

/// Where `pattern` starts if `text` has it right before `at`; a space in the pattern matches any
/// run of whitespace, since rustfmt may break a line there.
fn match_before(text: &str, at: usize, pattern: &str) -> Option<usize> {
    let mut rest = &text[..at];
    for (k, part) in pattern.rsplit(' ').enumerate() {
        rest = if k > 0 { rest.trim_end() } else { rest }.strip_suffix(part)?;
    }
    Some(rest.len())
}

/// Where `pattern` ends if `text` has it at `at`, with spaces matched as in [`match_before`].
fn match_at(text: &str, at: usize, pattern: &str) -> Option<usize> {
    let mut rest = &text[at..];
    for (k, part) in pattern.split(' ').enumerate() {
        rest = if k > 0 { rest.trim_start() } else { rest }.strip_prefix(part)?;
    }
    Some(text.len() - rest.len())
}

fn has_id(source: &str, marker: &str) -> bool {
    source
        .match_indices(marker)
        .any(|(at, _)| source[at + marker.len()..].starts_with(|c: char| c.is_ascii_digit()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mapped(source: &str, placeholders: &[(Range<usize>, &'static str, &str)]) -> Mapped {
        let found = placeholders
            .iter()
            .map(|(range, pre, post)| Placeholder::new(source, range.clone(), pre, post))
            .collect();
        Mapped::new(source, found)
    }

    #[test]
    fn whitespace_collapses_outside_literals_and_comments() {
        assert_eq!(normalize("var  \n "), "var ");
        assert_eq!(normalize("throws\n  E"), "throws E");
        assert_eq!(normalize("= \"a  b\""), "= \"a  b\"");
        assert_eq!(normalize("var /* c */ "), "var /* c */ ");
    }

    #[test]
    fn the_marker_avoids_ids_already_in_the_source() {
        let source = "fn f() { let _R1 = 1; var x = 2; }";
        let at = source.find("var").unwrap();
        let mapped = mapped(source, &[(at..at + 4, "let ", "")]);
        assert_eq!(mapped.text, "fn f() { let _R1 = 1; let _RR0x = 2; }");
        assert_eq!(mapped.restore(&mapped.text).unwrap(), source);
    }

    #[test]
    fn a_placeholder_inside_another_stays_part_of_its_text() {
        let source = "a ~(~T) b";
        let mapped = mapped(source, &[(2..7, "", "!"), (4..5, "", "::")]);
        assert_eq!(mapped.text, "a _R0! b");
        assert_eq!(mapped.restore("a _R0!\nb").unwrap(), "a ~(~T)\nb");
    }

    #[test]
    fn a_space_in_a_placeholder_matches_the_whitespace_rustfmt_puts_there() {
        let source = "{ throw e }";
        let mapped = mapped(source, &[(2..8, "break '", " "), (9..9, "; ", "")]);
        assert_eq!(mapped.text, "{ break '_R0 e; _R1 }");
        let formatted = "{\n    break '_R0\n        e;\n    _R1\n}\n";
        assert_eq!(mapped.restore(formatted).unwrap(), "{\n    throw e\n}\n");
    }

    #[test]
    fn a_moved_or_lost_placeholder_is_an_error_at_the_sugar() {
        let source = "x var y";
        let mapped = mapped(source, &[(2..6, "let ", "")]);
        assert_eq!(mapped.restore("x y").unwrap_err().0, 2);
        assert!(
            mapped
                .restore("x lets _R0y")
                .unwrap_err()
                .1
                .contains("moved")
        );
        assert!(
            mapped
                .restore("let _R0y let _R0y")
                .unwrap_err()
                .1
                .contains("moved")
        );
        assert_eq!(mapped.restore("x let\n_R0y").unwrap(), source);
    }

    #[test]
    fn offsets_in_the_mapped_text_lead_back_to_the_source() {
        let source = "a ~T b";
        let mapped = mapped(source, &[(2..3, "", "::")]);
        assert_eq!(mapped.text, "a _R0::T b");
        assert_eq!(mapped.source_offset(1), 1);
        assert_eq!(mapped.source_offset(4), 2);
        assert_eq!(mapped.source_offset(9), 5);
    }
}
