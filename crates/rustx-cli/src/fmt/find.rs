//! Finds the sugar spellings of a `.rustx` source and picks a valid-Rust placeholder for each.
//!
//! Detection follows the `//!` rules of the passes in `rustx-core`, simplified: a spelling found
//! where a pass would not see sugar is harmless, because the placeholder turns back into exactly
//! the text it replaced, and one missed makes rustfmt fail instead of changing the file.

use std::collections::HashSet;
use std::ops::Range;

use proc_macro2::{Delimiter, Spacing, TokenStream, TokenTree};
use syn::visit_mut::{self, VisitMut};

use super::mapped::Placeholder;

/// Identifiers that follow a name in valid Rust, so a prefix keyword before them is a name.
const NOT_OPERAND: &[&str] = &["as", "else", "for", "if", "in", "where"];
const PREFIXES: &[&str] = &["iter", "rc", "weak", "shared"];

/// Every sugar spelling of `tokens`, the lexed `source`, given `file`, its expansion.
pub(super) fn find(source: &str, tokens: TokenStream, file: &mut syn::File) -> Vec<Placeholder> {
    // `rc T` and `rc e` follow the same tokens; only the passes know which one they saw, and
    // `rc e` alone expands to a `new` call spanned at `rc`.
    let mut news = News(HashSet::new());
    news.visit_file_mut(file);
    let (rc_values, found) = (news.0, Vec::new());
    let mut finder = Finder {
        source,
        rc_values,
        found,
    };
    finder.walk(tokens, Delimiter::None);
    finder.found
}

struct News(HashSet<Range<usize>>);

impl News {
    fn tokens(&mut self, stream: TokenStream) {
        for tree in stream {
            match tree {
                TokenTree::Group(group) => self.tokens(group.stream()),
                TokenTree::Ident(mut ident) => self.visit_ident_mut(&mut ident),
                _ => {}
            }
        }
    }
}

impl VisitMut for News {
    fn visit_ident_mut(&mut self, ident: &mut syn::Ident) {
        if ident == "new" {
            self.0.insert(ident.span().byte_range());
        }
    }

    fn visit_macro_mut(&mut self, mac: &mut syn::Macro) {
        self.tokens(mac.tokens.clone());
        visit_mut::visit_macro_mut(self, mac);
    }
}

struct Finder<'a> {
    source: &'a str,
    rc_values: HashSet<Range<usize>>,
    found: Vec<Placeholder>,
}

impl Finder<'_> {
    fn walk(&mut self, stream: TokenStream, delimiter: Delimiter) {
        let seq: Vec<TokenTree> = stream.into_iter().collect();
        for (i, tree) in seq.iter().enumerate() {
            if let TokenTree::Group(group) = tree {
                self.walk(group.stream(), group.delimiter());
            }
            self.detect(&seq, i, delimiter);
        }
    }

    fn detect(&mut self, seq: &[TokenTree], i: usize, delimiter: Delimiter) {
        let next = seq.get(i + 1);
        match &seq[i] {
            TokenTree::Ident(ident) => match ident.to_string().as_str() {
                // The id is glued to the name, which keeps the width, unless the name is raw.
                "var" | "shared" if statement_start(seq, i) && operand(next) => {
                    let raw = next.is_some_and(|name| name.to_string().starts_with("r#"));
                    self.head(seq, i, "let ", if raw { " @ " } else { "" });
                }
                "throw" if expression_start(seq, i) && (operand(next) || literal(next)) => {
                    self.value(seq, i, delimiter);
                }
                "rc" if plain_name(seq, i) && self.rc_values.contains(&range(&seq[i])) => {
                    self.value(seq, i, delimiter);
                }
                "iter" | "rc" | "weak" | "shared" | "into" | "from"
                    if takes_type(seq, i, delimiter) =>
                {
                    self.head(seq, i, "&'", " ");
                }
                "fn" => {
                    self.throws(seq, i);
                    self.parameters(seq, i);
                }
                "bound" | "derive" => self.preset(seq, i),
                "move" => self.capture_list(seq, i),
                _ => {}
            },
            TokenTree::Group(group) if group.delimiter() == Delimiter::Parenthesis => {
                self.named_arguments(seq, i, group);
            }
            TokenTree::Punct(punct) => match punct.as_char() {
                '~' => {
                    let post = match next {
                        Some(TokenTree::Ident(_)) => "::",
                        Some(TokenTree::Group(_)) => "!",
                        _ => "",
                    };
                    self.push(range(&seq[i]), "", post);
                }
                '#' if dyn_attribute(next) => {
                    let at = range(&seq[i]).start..range(&seq[i + 1]).end;
                    self.push(at, "#[", "]");
                }
                '.' => self.turbofish(seq, i),
                '&' => self.borrow_lifetime(seq, i),
                _ => {}
            },
            _ => {}
        }
    }

    /// A sugar keyword (and the space after it) replaced by `pre`, the id and `post`.
    fn head(&mut self, seq: &[TokenTree], i: usize, pre: &'static str, post: &str) {
        if let Some(next) = seq.get(i + 1) {
            self.push(range(&seq[i]).start..range(next).start, pre, post);
        }
    }

    /// `throw e` and `rc e` become `break 'id e`. rustfmt ends a block's final `break` with `;`,
    /// so a value that ends its block gets `; id` after the block's last token, which keeps the
    /// `break` from being last and comes off again with that `;`.
    fn value(&mut self, seq: &[TokenTree], i: usize, delimiter: Delimiter) {
        self.head(seq, i, "break '", " ");
        let block = matches!(delimiter, Delimiter::Brace | Delimiter::None);
        if block && statement_start(seq, i) && !seq[i..].iter().any(|t| is_punct(Some(t), ';')) {
            let end = seq.last().map_or(0, |last| range(last).end);
            self.push(end..end, "; ", "");
        }
    }

    /// `x: T: Bound` and `x: T = v` in the parameters of `fn name<..>(..)`: the suffix from the
    /// second top-level `:` or the top-level `=` leaves, and an attribute stands in for it.
    fn parameters(&mut self, seq: &[TokenTree], i: usize) {
        let Some(TokenTree::Group(list)) = parameter_list(seq, i).map(|at| &seq[at]) else {
            return;
        };
        let params: Vec<TokenTree> = list.stream().into_iter().collect();
        let (mut start, mut depth) = (0, 0);
        for at in 0..=params.len() {
            if at == params.len() || (depth == 0 && is_punct(params.get(at), ',')) {
                self.parameter(&params[start..at]);
                start = at + 1;
            } else {
                depth = angle_depth(&params, at, depth);
            }
        }
    }

    fn parameter(&mut self, param: &[TokenTree]) {
        let (mut depth, mut colons) = (0, 0);
        for at in 0..param.len() {
            depth = angle_depth(param, at, depth);
            if depth > 0 {
                continue;
            }
            let colon = single_colon(param, at + 1);
            colons += usize::from(colon);
            if at > 0 && ((colon && colons == 2) || lone_equals(param, at)) {
                // The cut starts right after the type, so no blank is left before the comma.
                let (first, last) = (range(&param[0]), range(&param[param.len() - 1]));
                let suffix = range(&param[at - 1]).end..last.end;
                let cut = Placeholder::suffix(self.source, first.start, suffix, !colon);
                self.found.extend(cut);
                return;
            }
        }
    }

    /// `bound Name = ..;` and `derive Name = ..;` stay as written behind a macro-call item.
    fn preset(&mut self, seq: &[TokenTree], i: usize) {
        let named = matches!(seq.get(i + 1), Some(TokenTree::Ident(_)));
        if !named || !lone_equals(seq, i + 2) {
            return;
        }
        let public = |k: usize| is_named(seq.get(k), "pub").then_some(k);
        let start = public(i.wrapping_sub(1))
            .or_else(|| {
                public(i.wrapping_sub(2)).filter(|_| group(seq.get(i - 1), Delimiter::Parenthesis))
            })
            .unwrap_or(i);
        if !statement_start(seq, start) {
            return;
        }
        if let Some(end) = (i..seq.len()).find(|&k| is_punct(seq.get(k), ';')) {
            self.push(range(&seq[start]).start..range(&seq[end]).end, "", "!();");
        }
    }

    /// `move[..] |x|` keeps a closure: the list and the first `|` become `|id, `, and with
    /// `||` the list and both bars become `|id|`, the id padded to the width of the list.
    fn capture_list(&mut self, seq: &[TokenTree], i: usize) {
        if !group(seq.get(i + 1), Delimiter::Bracket) || !is_punct(seq.get(i + 2), '|') {
            return;
        }
        let empty = is_joint_punct(seq.get(i + 2)) && is_punct(seq.get(i + 3), '|');
        let end = range(&seq[i + 2 + usize::from(empty)]).end;
        let at = range(&seq[i + 1]).start..end;
        let pad = "_".repeat(at.len().saturating_sub(6));
        let post = if empty {
            format!("{pad}|")
        } else {
            format!("{pad}, ")
        };
        self.push(at, " |", &post);
    }

    /// `f(name: v)` in a call: a parenthesised group after a name not declared by `fn`, or after
    /// another `(..)` or `[..]`. `name:` becomes an assignment to the id, padded to the name.
    fn named_arguments(&mut self, seq: &[TokenTree], i: usize, args: &proc_macro2::Group) {
        let call = match seq.get(i.wrapping_sub(1)) {
            Some(TokenTree::Ident(name)) => {
                name != "fn" && !is_named(seq.get(i.wrapping_sub(2)), "fn")
            }
            Some(TokenTree::Group(before)) => before.delimiter() != Delimiter::Brace,
            _ => false,
        };
        if !call {
            return;
        }
        let args: Vec<TokenTree> = args.stream().into_iter().collect();
        let mut start = 0;
        while start < args.len() {
            if let Some(TokenTree::Ident(name)) = args.get(start)
                && single_colon(&args, start + 2)
            {
                let at = range(&args[start]).start..range(&args[start + 1]).end;
                let pad = "_".repeat(name.to_string().len());
                self.push(at, "", &format!("{pad} ="));
            }
            start = argument_end(&args, start) + 1;
        }
    }

    fn push(&mut self, range: Range<usize>, pre: &'static str, post: &str) {
        self.found
            .push(Placeholder::new(self.source, range, pre, post));
    }

    /// `fn name<..>(..) -> T throws E` loses `throws E` to a comment right after `T`, padded to
    /// the width of the sugar so that rustfmt breaks the line where it would for the sugar.
    fn throws(&mut self, seq: &[TokenTree], i: usize) {
        let (mut depth, mut arrow, mut throws) = (0, false, None);
        for j in i + 1..seq.len() {
            depth = angle_depth(seq, j, depth);
            let tree = seq.get(j);
            if depth > 0 {
            } else if [';', ',', '='].iter().any(|&ch| is_punct(tree, ch))
                || group(tree, Delimiter::Brace)
                || is_named(tree, "where")
            {
                if let Some(t) = throws {
                    let at = range(&seq[t]).start..range(&seq[j - 1]).end;
                    let post = format!("{}*/", "_".repeat(at.len().saturating_sub(7)));
                    self.push(at, "/*", &post);
                }
                return;
            } else if after_arrow(seq, j) {
                arrow = true;
            } else if arrow && is_named(tree, "throws") {
                throws = throws.or(Some(j));
            }
        }
    }

    /// `.m<T>(` keeps its arguments; the method name carries the id and gains `::`.
    fn turbofish(&mut self, seq: &[TokenTree], i: usize) {
        let (Some(TokenTree::Ident(method)), open) = (seq.get(i + 1), i + 2) else {
            return;
        };
        if is_punct(seq.get(open), '<')
            && !is_joint_punct(seq.get(open))
            && let Some(close) = matching_close(seq, open)
            && group(seq.get(close + 1), Delimiter::Parenthesis)
        {
            let at = range(&seq[i + 1]).start..range(&seq[open]).end;
            self.push(at, "", &format!("{method}::<"));
        }
    }

    /// `&from(a) T` and `&mut from(a) T` take a lifetime that carries the id.
    fn borrow_lifetime(&mut self, seq: &[TokenTree], i: usize) {
        let mutable = is_named(seq.get(i + 1), "mut");
        let from = i + 1 + usize::from(mutable);
        if !is_named(seq.get(from), "from") || !group(seq.get(from + 1), Delimiter::Parenthesis) {
            return;
        }
        let ty = seq.get(from + 2);
        if let Some(ty) = ty.filter(|_| operand(ty) || (after_arrow(seq, i) && type_punct(ty))) {
            let post = if mutable { " mut " } else { " " };
            let at = range(&seq[i + 1]).start..range(ty).start;
            self.push(at, "'", post);
        }
    }
}

fn range(tree: &TokenTree) -> Range<usize> {
    tree.span().byte_range()
}

fn is_punct(tree: Option<&TokenTree>, ch: char) -> bool {
    matches!(tree, Some(TokenTree::Punct(p)) if p.as_char() == ch)
}

fn is_joint_punct(tree: Option<&TokenTree>) -> bool {
    matches!(tree, Some(TokenTree::Punct(p)) if p.spacing() == Spacing::Joint)
}

fn is_named(tree: Option<&TokenTree>, name: &str) -> bool {
    matches!(tree, Some(TokenTree::Ident(i)) if i == name)
}

fn group(tree: Option<&TokenTree>, delimiter: Delimiter) -> bool {
    matches!(tree, Some(TokenTree::Group(g)) if g.delimiter() == delimiter)
}

/// An identifier that can start an operand: a value after `throw`, a type after a prefix.
fn operand(tree: Option<&TokenTree>) -> bool {
    matches!(tree, Some(TokenTree::Ident(i)) if !NOT_OPERAND.iter().any(|k| i == k))
}

fn literal(tree: Option<&TokenTree>) -> bool {
    matches!(tree, Some(TokenTree::Literal(_)))
}

/// A prefix before a type, `into` before a parameter type, or `from` before a variant's type.
fn takes_type(seq: &[TokenTree], i: usize, delimiter: Delimiter) -> bool {
    match seq[i].to_string().as_str() {
        "into" => single_colon(seq, i) && type_follows(seq, i),
        "from" => {
            let next = seq.get(i + 1);
            i == 0 && delimiter == Delimiter::Parenthesis && (operand(next) || type_punct(next))
        }
        _ => plain_name(seq, i) && type_follows(seq, i),
    }
}

/// A token other than a name that starts a type: `&`, `*`, a tuple, array or slice.
fn type_punct(tree: Option<&TokenTree>) -> bool {
    is_punct(tree, '&') || is_punct(tree, '*') || matches!(tree, Some(TokenTree::Group(_)))
}

fn dyn_attribute(tree: Option<&TokenTree>) -> bool {
    matches!(tree, Some(TokenTree::Group(g)) if g.delimiter() == Delimiter::Bracket
        && is_named(g.stream().into_iter().next().as_ref(), "dyn"))
}

fn after_arrow(seq: &[TokenTree], i: usize) -> bool {
    i >= 2 && is_punct(seq.get(i - 1), '>') && is_punct(seq.get(i - 2), '-')
}

/// A name, not a field, path segment, label, lifetime or macro variable.
fn plain_name(seq: &[TokenTree], i: usize) -> bool {
    let before = seq.get(i.wrapping_sub(1));
    i == 0
        || !(['\'', '$', '.', '#'].iter().any(|&ch| is_punct(before, ch))
            || is_punct(before, ':') && !single_colon(seq, i))
}

/// After a prefix keyword: a type, or after `->` or another prefix also a group or `&`/`*`.
fn type_follows(seq: &[TokenTree], i: usize) -> bool {
    let next = seq.get(i + 1);
    let after_prefix = i > 0 && PREFIXES.iter().any(|k| is_named(seq.get(i - 1), k));
    operand(next)
        || is_punct(next, '~')
        || ((after_arrow(seq, i) || after_prefix) && type_punct(next))
}

/// The comma after the argument that starts at `from`, past the parameters of a closure, which
/// hold commas and `name: Type` of their own.
fn argument_end(args: &[TokenTree], from: usize) -> usize {
    let mut at = from;
    while is_named(args.get(at), "move") || is_named(args.get(at), "async") {
        at += 1;
    }
    if is_punct(args.get(at), '|') {
        at = if is_joint_punct(args.get(at)) && is_punct(args.get(at + 1), '|') {
            at + 2
        } else {
            (at + 1..args.len())
                .find(|&k| is_punct(args.get(k), '|'))
                .map_or(args.len(), |close| close + 1)
        };
    }
    (at..args.len())
        .find(|&k| is_punct(args.get(k), ','))
        .unwrap_or(args.len())
}

/// A `=` at `i` that is neither part of `==`, `=>`, `<=` and the like nor followed by one.
fn lone_equals(seq: &[TokenTree], i: usize) -> bool {
    let next = seq.get(i + 1);
    is_punct(seq.get(i), '=')
        && !is_joint_punct(seq.get(i.wrapping_sub(1)))
        && !(is_joint_punct(seq.get(i)) && (is_punct(next, '=') || is_punct(next, '>')))
}

/// The parameter group when `fn` stands at `i`: `fn name(..)` or `fn name<..>(..)`.
fn parameter_list(seq: &[TokenTree], i: usize) -> Option<usize> {
    if !matches!(seq.get(i + 1), Some(TokenTree::Ident(_))) {
        return None;
    }
    let mut at = i + 2;
    if is_punct(seq.get(at), '<') {
        at = matching_close(seq, at)? + 1;
    }
    group(seq.get(at), Delimiter::Parenthesis).then_some(at)
}

/// A single `:` right before `i`, not part of `::`.
fn single_colon(seq: &[TokenTree], i: usize) -> bool {
    let before = |k: usize| seq.get(i.wrapping_sub(k));
    is_punct(before(1), ':')
        && !is_joint_punct(before(1))
        && !(is_punct(before(2), ':') && is_joint_punct(before(2)))
}

fn statement_start(seq: &[TokenTree], i: usize) -> bool {
    let before = |k: usize| seq.get(i.wrapping_sub(k));
    i == 0
        || is_punct(before(1), ';')
        || group(before(1), Delimiter::Brace)
        || (group(before(1), Delimiter::Bracket)
            && (is_punct(before(2), '#') || is_punct(before(2), '!')))
}

/// A statement start, or right after `=>` or a lone `=`.
fn expression_start(seq: &[TokenTree], i: usize) -> bool {
    let before = |k: usize| seq.get(i.wrapping_sub(k));
    statement_start(seq, i)
        || (is_punct(before(1), '>') && is_punct(before(2), '='))
        || (is_punct(before(1), '=') && !is_joint_punct(before(1)) && !is_joint_punct(before(2)))
}

/// The angle-bracket depth after `seq[j]`; the `>` of `->` and `=>` closes nothing.
fn angle_depth(seq: &[TokenTree], j: usize, depth: usize) -> usize {
    let before = seq.get(j.wrapping_sub(1));
    let arrow = is_joint_punct(before) && (is_punct(before, '-') || is_punct(before, '='));
    match &seq[j] {
        TokenTree::Punct(p) if p.as_char() == '<' => depth + 1,
        TokenTree::Punct(p) if p.as_char() == '>' && !arrow => depth.saturating_sub(1),
        _ => depth,
    }
}

/// The `>` that closes the `<` at `open`, if it closes before a `;`.
fn matching_close(seq: &[TokenTree], open: usize) -> Option<usize> {
    let mut depth = 0;
    for j in open..seq.len() {
        if is_punct(seq.get(j), ';') {
            return None;
        }
        let was = depth;
        depth = angle_depth(seq, j, depth);
        if was > 0 && depth == 0 {
            return Some(j);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::super::mapped::Mapped;
    use super::*;

    /// The mapped text of `source`, after checking that it parses and that it maps back.
    fn round_trip(source: &str) -> String {
        let tokens: TokenStream = syn::parse_str(source).unwrap();
        let mut file = rustx_core::try_expand(tokens.clone()).unwrap();
        let mapped = Mapped::new(source, find(source, tokens, &mut file));
        if let Err(err) = syn::parse_file(&mapped.text) {
            panic!("{err}: {}", mapped.text);
        }
        assert_eq!(mapped.restore(&mapped.text).unwrap(), source);
        mapped.text
    }

    #[test]
    fn var_and_shared_bindings_become_let_with_the_id_glued_to_the_name() {
        let text = round_trip("fn f() { var x = 1; x += 1; shared s = 2; }");
        assert_eq!(text, "fn f() { let _R0x = 1; x += 1; let _R1s = 2; }");
    }

    #[test]
    fn a_raw_binding_name_is_bound_with_an_at_pattern() {
        let text = round_trip("fn f() { var r#x = 1; r#x += 1; }");
        assert_eq!(text, "fn f() { let _R0 @ r#x = 1; r#x += 1; }");
    }

    #[test]
    fn throw_becomes_a_labelled_break_closed_off_at_the_block_end() {
        let text = round_trip("fn f() -> u8 throws { if g() { throw \"no\" } 1 }");
        assert_eq!(
            text,
            "fn f() -> u8 /*_R0*/ { if g() { break '_R1 \"no\"; _R2 } 1 }"
        );
    }

    #[test]
    fn rc_values_and_rc_types_get_different_placeholders() {
        let text = round_trip("fn f() -> rc u8 { rc 1 }");
        assert_eq!(text, "fn f() -> &'_R0 u8 { break '_R1 1; _R2 }");
    }

    #[test]
    fn prefixes_and_into_become_lifetimes_of_a_reference() {
        let source = "fn f(xs: iter u8, w: weak str, s: shared u8, n: into String) {}";
        let text = round_trip(source);
        let expected = "fn f(xs: &'_R0 u8, w: &'_R1 str, s: &'_R2 u8, n: &'_R3 String) {}";
        assert_eq!(text, expected);
    }

    #[test]
    fn from_variants_and_from_lifetimes_become_lifetimes() {
        let text = round_trip("enum E { Io(from std::io::Error) }");
        assert_eq!(text, "enum E { Io(&'_R0 std::io::Error) }");
        let text = round_trip("fn f(x: &mut u8) -> &mut from(x) u8 { todo!() }");
        assert_eq!(text, "fn f(x: &mut u8) -> &'_R0 mut u8 { todo!() }");
    }

    #[test]
    fn tilde_becomes_a_path_and_dyn_attributes_an_attribute() {
        let source = "#[dyn] trait T {} fn f(a: ~T, b: Vec<~T>) {}";
        let text = round_trip(source);
        let expected = "#[_R0] trait T {} fn f(a: _R1::T, b: Vec<_R2::T>) {}";
        assert_eq!(text, expected);
    }

    #[test]
    fn method_generics_become_a_turbofish_on_a_renamed_method() {
        let text = round_trip("fn f() { let v = x.collect<Vec<u8>>(); }");
        assert_eq!(text, "fn f() { let v = x._R0collect::<Vec<u8>>(); }");
    }

    #[test]
    fn a_long_throws_type_gets_a_comment_as_wide_as_itself() {
        let text = round_trip("fn f() -> u8 throws std::io::Error { 1 }");
        assert_eq!(text, "fn f() -> u8 /*_R0______________*/ { 1 }");
    }

    #[test]
    fn presets_become_macro_call_items() {
        let source = "pub bound Key = Clone + Send; derive Id = Debug, Clone;";
        assert_eq!(round_trip(source), "_R0!(); _R1!();");
    }

    #[test]
    fn inline_bounds_and_defaults_leave_for_an_attribute_on_their_parameter() {
        let source = "fn run(x: T: Clone + Send, (a, b): (u8, u8) = (1, 2), s: &str = \"s\") {}";
        let expected = "fn run(#[_R0_______] x: T, #[_R1__] (a, b): (u8, u8), #[_R2] s: &str) {}";
        assert_eq!(round_trip(source), expected);
    }

    #[test]
    fn named_arguments_become_assignments_but_closure_parameters_stay() {
        let source = "fn f(port: u8 = 1) {} fn g() { f(port: 2); h(|a: u8, b: u8| a); }";
        let expected = "fn f(#[_R0] port: u8) {} fn g() { f(_R1____ = 2); h(|a: u8, b: u8| a); }";
        assert_eq!(round_trip(source), expected);
    }

    #[test]
    fn capture_lists_become_a_closure_parameter_as_wide_as_the_list() {
        let source = "fn f() { let c = move[a.clone()] || a; let d = move[b] |x: u8| x; }";
        let expected = "fn f() { let c = move |_R0________| a; let d = move |_R1, x: u8| x; }";
        assert_eq!(round_trip(source), expected);
    }

    #[test]
    fn valid_rust_with_sugar_keywords_as_names_is_left_alone() {
        let source = "fn f(rc: u8, iter: u8) -> u8 { let throw = rc; a.iter < b; throw }";
        assert_eq!(round_trip(source), source);
    }
}
