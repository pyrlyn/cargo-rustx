//! Placeholders of a `#[display("..")]` string, rewritten into explicit positional arguments.
//!
//! The generated `write!` passes only the fields the string names, because `format_args!`
//! rejects unused arguments; so every placeholder is renumbered to its field's place in that
//! list (`"{1} {name}"` → `"{0} {1}"` with arguments field 1 and `name`). Explicit arguments
//! also keep the bindings out of reach of the user's string, which implicit capture would need.

use std::fmt::Write as _;

/// The rewritten string and the fields it formats, in argument order.
///
/// `field` resolves a placeholder's argument (`0`, `name`, or the implicit index of `{}`
/// written as a number) to the field's position in the variant.
pub(super) fn rewrite(
    text: &str,
    variant: &str,
    field: impl Fn(&str) -> Option<usize>,
) -> Result<(String, Vec<usize>), String> {
    let mut out = String::with_capacity(text.len());
    let mut used: Vec<usize> = Vec::new();
    // `{}` takes the next implicit position, counted apart from explicit `{0}`, as in `format!`.
    let mut implicit = 0;
    let mut rest = text;
    while let Some(at) = rest.find(['{', '}']) {
        out.push_str(&rest[..at]);
        let tail = &rest[at..];
        // A stray `}` or an unclosed `{` is copied, so rustc reports it on the user's string.
        let close = tail.find('}').filter(|_| tail.starts_with('{'));
        let Some(close) = close.filter(|_| !tail.starts_with("{{")) else {
            let escape = if tail.starts_with("{{") || tail.starts_with("}}") {
                2
            } else {
                1
            };
            out.push_str(&tail[..escape]);
            rest = &tail[escape..];
            continue;
        };
        let placeholder = &tail[1..close];
        let (arg, spec) = match placeholder.split_once(':') {
            Some((arg, spec)) => (arg, Some(spec)),
            None => (placeholder, None),
        };
        // A `$` past the first character is a width or precision argument, not a fill.
        if spec.is_some_and(|spec| spec.contains(".*") || spec.chars().skip(1).any(|c| c == '$')) {
            return Err(format!(
                "`{{{placeholder}}}`: width and precision arguments (`$`, `.*`) are not supported \
                 in `#[display]`; write them as numbers"
            ));
        }
        let arg = if arg.is_empty() {
            implicit += 1;
            (implicit - 1).to_string()
        } else {
            arg.to_owned()
        };
        let Some(index) = field(&arg) else {
            return Err(format!("`{{{placeholder}}}` names no field of `{variant}`"));
        };
        let position = used.iter().position(|&u| u == index).unwrap_or_else(|| {
            used.push(index);
            used.len() - 1
        });
        let _ = write!(out, "{{{position}");
        if let Some(spec) = spec {
            out.push(':');
            out.push_str(spec);
        }
        out.push('}');
        rest = &tail[close + 1..];
    }
    out.push_str(rest);
    Ok((out, used))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Resolves like a variant `V(a, b, c)` with a named alias: digits below 3 and `name` → 1.
    fn tuple(arg: &str) -> Option<usize> {
        match arg {
            "name" => Some(1),
            _ => arg.parse().ok().filter(|&i| i < 3),
        }
    }

    fn rewritten(text: &str) -> Result<(String, Vec<usize>), String> {
        rewrite(text, "V", tuple)
    }

    #[test]
    fn placeholders_are_renumbered_to_the_fields_they_name() {
        assert_eq!(
            rewritten("io error: {0}").unwrap(),
            ("io error: {0}".to_owned(), vec![0])
        );
        assert_eq!(
            rewritten("{2:>4} then {name:?} and {2}").unwrap(),
            ("{0:>4} then {1:?} and {0}".to_owned(), vec![2, 1])
        );
    }

    #[test]
    fn implicit_placeholders_count_apart_from_explicit_ones() {
        assert_eq!(
            rewritten("{} {0} {:#?}").unwrap(),
            ("{0} {0} {1:#?}".to_owned(), vec![0, 1])
        );
    }

    #[test]
    fn escapes_fills_and_text_without_placeholders_are_kept() {
        assert_eq!(
            rewritten("{{literal}} {1:$>5} }} plain").unwrap(),
            ("{{literal}} {0:$>5} }} plain".to_owned(), vec![1])
        );
        assert_eq!(
            rewritten("no fields").unwrap(),
            ("no fields".to_owned(), vec![])
        );
    }

    #[test]
    fn malformed_strings_are_left_for_rustc() {
        assert_eq!(
            rewritten("a } b {0").unwrap(),
            ("a } b {0".to_owned(), vec![])
        );
    }

    #[test]
    fn a_placeholder_naming_no_field_is_an_error() {
        assert_eq!(rewritten("{3}").unwrap_err(), "`{3}` names no field of `V`");
        assert_eq!(
            rewritten("{other:?}").unwrap_err(),
            "`{other:?}` names no field of `V`"
        );
        assert_eq!(
            rewritten("{} {} {} {}").unwrap_err(),
            "`{}` names no field of `V`"
        );
    }

    #[test]
    fn width_and_precision_arguments_are_an_error() {
        for bad in ["{0:1$}", "{:.*}", "{0:>w$}"] {
            assert!(
                rewritten(bad).unwrap_err().contains("not supported"),
                "{bad}"
            );
        }
    }
}
