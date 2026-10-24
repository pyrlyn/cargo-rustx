//! Parameters of `fn` signatures in a token stream, one at a time.
//!
//! Separate because several passes rewrite parameters (inline bounds, `into T`, defaults) and
//! must agree on what a parameter list is: only the parenthesised group right after `fn name`
//! or `fn name<..>`, so closures, struct fields and fn-pointer types are never touched.

use proc_macro2::{Delimiter, Group, TokenStream, TokenTree};

use super::angle::{angle_depth, matching_close};
use super::map_sequences;
use super::punct::{is_joint, is_punct};

/// Applies `rewrite` to every parameter of every `fn` signature in `tokens`, nested ones too.
///
/// Parameters are split at commas outside `<..>`; the commas themselves are kept. `rewrite`
/// returns the replacement tokens, or the parameter unchanged.
pub(crate) fn map_fn_parameters<F>(tokens: TokenStream, rewrite: &mut F) -> syn::Result<TokenStream>
where
    F: FnMut(&[TokenTree]) -> syn::Result<Vec<TokenTree>>,
{
    map_sequences(tokens, &mut |mut seq: Vec<TokenTree>| {
        for i in 0..seq.len() {
            let Some(at) = parameter_list(&seq, i) else {
                continue;
            };
            if let TokenTree::Group(group) = &seq[at] {
                let mut rebuilt =
                    Group::new(Delimiter::Parenthesis, rewrite_parameters(group, rewrite)?);
                rebuilt.set_span(group.span());
                seq[at] = TokenTree::Group(rebuilt);
            }
        }
        Ok(seq)
    })
}

/// Index of the parameter group when `fn` stands at `i`: `fn name(..)` or `fn name<..>(..)`.
#[must_use]
pub fn parameter_list(seq: &[TokenTree], i: usize) -> Option<usize> {
    if !matches!(seq.get(i), Some(TokenTree::Ident(word)) if word == "fn")
        || !matches!(seq.get(i + 1)?, TokenTree::Ident(_))
    {
        return None;
    }
    let mut at = i + 2;
    if is_punct(seq.get(at)?, '<') {
        at = matching_close(seq, at)? + 1;
    }
    match seq.get(at)? {
        TokenTree::Group(group) if group.delimiter() == Delimiter::Parenthesis => Some(at),
        _ => None,
    }
}

/// Index of the comma that ends the argument starting at `from`, or the end of `args`.
#[must_use]
pub fn argument_end(args: &[TokenTree], from: usize) -> usize {
    let mut at = from;
    while matches!(args.get(at), Some(TokenTree::Ident(word)) if word == "move" || word == "async")
    {
        let capture = matches!(&args[at], TokenTree::Ident(word) if word == "move");
        at += 1;
        if capture
            && matches!(args.get(at), Some(TokenTree::Group(group)) if group.delimiter() == Delimiter::Bracket)
        {
            at += 1;
        }
    }
    // A closure's parameters may hold commas and `name: Type`; they end at the next `|`.
    if args.get(at).is_some_and(|tree| is_punct(tree, '|')) {
        let empty =
            is_joint(&args[at], '|') && args.get(at + 1).is_some_and(|tree| is_punct(tree, '|'));
        at = if empty {
            at + 2
        } else {
            args[at + 1..]
                .iter()
                .position(|tree| is_punct(tree, '|'))
                .map_or(args.len(), |close| at + close + 2)
        };
    }
    let at = at.min(args.len());
    args[at..]
        .iter()
        .position(|tree| is_punct(tree, ','))
        .map_or(args.len(), |comma| at + comma)
}

fn rewrite_parameters<F>(group: &Group, rewrite: &mut F) -> syn::Result<TokenStream>
where
    F: FnMut(&[TokenTree]) -> syn::Result<Vec<TokenTree>>,
{
    let seq: Vec<TokenTree> = group.stream().into_iter().collect();
    let mut out = Vec::with_capacity(seq.len());
    let mut start = 0;
    let mut depth = 0;
    for at in 0..=seq.len() {
        let comma = seq.get(at).is_some_and(|tree| is_punct(tree, ','));
        if at == seq.len() || (comma && depth == 0) {
            out.extend(rewrite(&seq[start..at])?);
            out.extend(seq.get(at).cloned());
            start = at + 1;
        } else {
            depth = angle_depth(&seq, at, depth);
        }
    }
    Ok(out.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parameters(source: &str) -> Vec<String> {
        let mut seen = Vec::new();
        map_fn_parameters(source.parse().unwrap(), &mut |param: &[TokenTree]| {
            seen.push(param.iter().cloned().collect::<TokenStream>().to_string());
            Ok(param.to_vec())
        })
        .unwrap();
        seen
    }

    #[test]
    fn parameters_split_at_commas_outside_angle_brackets() {
        assert_eq!(
            parameters("fn f<A: Fn() -> u8>(m: HashMap<K, V>, n: u8) {}"),
            ["m : HashMap < K , V >", "n : u8"]
        );
    }

    #[test]
    fn closures_fn_pointers_and_struct_fields_are_not_parameter_lists() {
        assert!(
            parameters("struct S { f: fn(u8) -> u8 } fn g() { let c = |x: u8| x; }")
                .iter()
                .all(String::is_empty)
        );
    }

    #[test]
    fn nested_fns_are_visited() {
        assert_eq!(
            parameters("mod m { impl S { fn f(&self, x: u8) {} } }"),
            ["& self", "x : u8"]
        );
    }

    #[test]
    fn errors_from_rewrite_propagate() {
        let err = map_fn_parameters(
            "fn f(x: u8) {}".parse().unwrap(),
            &mut |_: &[TokenTree]| Err(syn::Error::new(proc_macro2::Span::call_site(), "nope")),
        )
        .unwrap_err();
        assert_eq!(err.to_string(), "nope");
    }
}
