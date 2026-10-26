//! Reading a `write!(f, "..", args)` call from its source text, and turning its format string
//! into a `#[display("..")]` string.
//!
//! Only the simple shape converts: a formatter name, one plain string literal, and arguments that
//! are bare names (`x`) or named bare names (`name = x`). Anything else (an expression argument,
//! a raw string, a `\u{..}` escape, a width or precision argument) makes [`parse`] or
//! [`WriteCall::rewrite`] return `None`, and the impl stays as written.

use std::fmt::Write as _;

/// The field a binding names: the position in a tuple variant, or the name in a struct variant.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FieldRef {
    Index(usize),
    Named(String),
}

#[derive(Debug, PartialEq, Eq)]
pub struct WriteCall {
    /// The formatter written first, `f` in `write!(f, ..)`.
    pub target: String,
    /// The string literal between its quotes, escapes as written.
    pub literal: String,
    /// Positional arguments, each a bare name.
    pub positional: Vec<String>,
    /// `name = value` arguments.
    pub named: Vec<(String, String)>,
}

/// Parses `write!(target, "literal", args..)`, as the source of a `write!` call reads.
pub fn parse(text: &str) -> Option<WriteCall> {
    let rest = text.trim().strip_prefix("write")?.trim_start().strip_prefix('!')?.trim_start();
    let close = match rest.chars().next()? {
        '(' => ')',
        '[' => ']',
        '{' => '}',
        _ => return None,
    };
    let inner = rest.get(1..)?.strip_suffix(close)?.trim();
    let (target, rest) = inner.split_once(',')?;
    let target = target.trim();
    if !is_name(target) {
        return None;
    }
    let rest = rest.trim_start().strip_prefix('"')?;
    let mut end = None;
    let mut escaped = false;
    for (at, c) in rest.char_indices() {
        match (escaped, c) {
            (true, _) => escaped = false,
            (false, '\\') => escaped = true,
            (false, '"') => {
                end = Some(at);
                break;
            }
            _ => {}
        }
    }
    let end = end?;
    let literal = &rest[..end];
    // A `\u{..}` escape holds braces the placeholder scan would misread.
    if literal.contains("\\u") {
        return None;
    }
    let mut call = WriteCall {
        target: target.to_owned(),
        literal: literal.to_owned(),
        positional: Vec::new(),
        named: Vec::new(),
    };
    let args = rest[end + 1..].trim();
    if args.is_empty() {
        return Some(call);
    }
    for arg in args.strip_prefix(',')?.split(',') {
        let arg = arg.trim();
        if arg.is_empty() {
            continue;
        }
        match arg.split_once('=') {
            Some((name, value)) if is_name(name.trim()) && is_name(value.trim()) => {
                call.named.push((name.trim().to_owned(), value.trim().to_owned()));
            }
            None if is_name(arg) && call.named.is_empty() => call.positional.push(arg.to_owned()),
            _ => return None,
        }
    }
    Some(call)
}

/// The `#[display("..")]` string printing `text` as it is, as `f.write_str(text)` does: escaped
/// for a string literal, with its braces doubled.
pub fn literal_message(text: &str) -> String {
    let quoted = format!("{text:?}");
    let escaped = &quoted[1..quoted.len() - 1];
    escaped.replace('{', "{{").replace('}', "}}")
}

fn is_name(text: &str) -> bool {
    let mut chars = text.chars();
    chars.next().is_some_and(|c| c.is_alphabetic() || c == '_')
        && chars.all(|c| c.is_alphanumeric() || c == '_')
}

impl WriteCall {
    /// The literal with every placeholder naming the field its argument binds: `{0}` for a tuple
    /// field, `{key}` for a named one, as `#[display]` reads them. `field` maps a binding name to
    /// its field; a placeholder whose value is not a binding of the variant fails.
    pub fn rewrite(&self, field: impl Fn(&str) -> Option<FieldRef>) -> Option<String> {
        let mut out = String::with_capacity(self.literal.len());
        let mut implicit = 0;
        let mut rest = self.literal.as_str();
        while let Some(at) = rest.find(['{', '}']) {
            out.push_str(&rest[..at]);
            let tail = &rest[at..];
            if tail.starts_with("{{") || tail.starts_with("}}") {
                out.push_str(&tail[..2]);
                rest = &tail[2..];
                continue;
            }
            if tail.starts_with('}') {
                return None;
            }
            let close = tail.find('}')?;
            let placeholder = &tail[1..close];
            let (arg, spec) = match placeholder.split_once(':') {
                Some((arg, spec)) => (arg.trim(), Some(spec)),
                None => (placeholder.trim(), None),
            };
            if spec.is_some_and(|spec| spec.contains(".*") || spec.chars().skip(1).any(|c| c == '$')) {
                return None;
            }
            let value = if arg.is_empty() {
                implicit += 1;
                self.positional.get(implicit - 1)?.as_str()
            } else if let Ok(index) = arg.parse::<usize>() {
                self.positional.get(index)?.as_str()
            } else if let Some((_, value)) = self.named.iter().find(|(name, _)| name == arg) {
                value.as_str()
            } else if is_name(arg) {
                // An implicitly captured binding.
                arg
            } else {
                return None;
            };
            match field(value)? {
                FieldRef::Index(index) => {
                    let _ = write!(out, "{{{index}");
                }
                FieldRef::Named(name) => {
                    let _ = write!(out, "{{{name}");
                }
            }
            if let Some(spec) = spec {
                out.push(':');
                out.push_str(spec);
            }
            out.push('}');
            rest = &tail[close + 1..];
        }
        out.push_str(rest);
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tuple(name: &str) -> Option<FieldRef> {
        match name {
            "a" => Some(FieldRef::Index(0)),
            "b" => Some(FieldRef::Index(1)),
            _ => None,
        }
    }

    #[test]
    fn a_plain_write_call_is_read_with_its_arguments() {
        let call = parse(r#"write!(f, "x {} {b}", a, b = b,)"#).unwrap();
        assert_eq!(call.target, "f");
        assert_eq!(call.literal, "x {} {b}");
        assert_eq!(call.positional, ["a"]);
        assert_eq!(call.named, [("b".to_owned(), "b".to_owned())]);
    }

    #[test]
    fn expression_arguments_and_raw_strings_are_not_read() {
        for text in [
            r#"write!(f, "{}", self.0)"#,
            r#"write!(f, "{}", a.len())"#,
            r##"write!(f, r#"{}"#, a)"##,
            r#"write!(f, "\u{7b}", a)"#,
            r#"writeln!(f, "{}", a)"#,
        ] {
            assert_eq!(parse(text), None, "{text}");
        }
    }

    #[test]
    fn placeholders_are_renumbered_to_the_fields_they_bind() {
        let call = parse(r#"write!(f, "{} then {0:>4} and {b:?} {{lit}}", b, a)"#).unwrap();
        assert_eq!(call.rewrite(tuple).unwrap(), "{1} then {1:>4} and {1:?} {{lit}}");
        let call = parse(r#"write!(f, "{a}-{}", b)"#).unwrap();
        assert_eq!(call.rewrite(tuple).unwrap(), "{0}-{1}");
        let named = |name: &str| (name == "k").then(|| FieldRef::Named("key".to_owned()));
        let call = parse(r#"write!(f, "missing {}", k)"#).unwrap();
        assert_eq!(call.rewrite(named).unwrap(), "missing {key}");
    }

    #[test]
    fn a_written_str_is_escaped_into_a_format_string() {
        assert_eq!(literal_message("gone {x} \"q\""), "gone {{x}} \\\"q\\\"");
    }

    #[test]
    fn width_arguments_and_unknown_names_fail() {
        assert_eq!(parse(r#"write!(f, "{:1$}", a, b)"#).unwrap().rewrite(tuple), None);
        assert_eq!(parse(r#"write!(f, "{:.*}", a, b)"#).unwrap().rewrite(tuple), None);
        assert_eq!(parse(r#"write!(f, "{other}")"#).unwrap().rewrite(tuple), None);
    }
}
