//! Renders desugar and parse errors as `path:line:column: message` lines, the form editors and
//! terminals turn into links.

use std::path::Path;

use proc_macro2::Span;

/// One line per message of `err`, with 1-based line and column like rustc.
pub(crate) fn render(path: &Path, source: &str, err: syn::Error) -> String {
    let lines: Vec<String> = err
        .into_iter()
        .map(|err| {
            let (line, column) = location(err.span(), source);
            format!("{}:{line}:{column}: {err}", path.display())
        })
        .collect();
    lines.join("\n")
}

/// Spans with no source text are call-site spans, which the parser uses for the end of input.
pub(crate) fn is_call_site(span: Span) -> bool {
    span.source_text().is_none()
}

fn location(span: Span, source: &str) -> (usize, usize) {
    if is_call_site(span) {
        let line = source.lines().count().max(1);
        let column = source.lines().last().map_or(0, |last| last.chars().count());
        return (line, column + 1);
    }
    let start = span.start();
    (start.line, start.column + 1)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_str(source: &str, err: impl FnOnce(&syn::File) -> syn::Error) -> String {
        let file = syn::parse_file(source).unwrap();
        render(Path::new("dir/a.rs"), source, err(&file))
    }

    #[test]
    fn a_message_is_prefixed_with_path_line_and_one_based_column() {
        let source = "fn a() {}\n  fn b() {}\n";
        let rendered = render_str(source, |file| {
            syn::Error::new_spanned(&file.items[1], "bad")
        });
        assert_eq!(rendered, "dir/a.rs:2:3: bad");
    }

    #[test]
    fn combined_errors_get_one_line_each() {
        let source = "fn a() {}\nfn b() {}\n";
        let rendered = render_str(source, |file| {
            let mut err = syn::Error::new_spanned(&file.items[0], "first");
            err.combine(syn::Error::new_spanned(&file.items[1], "second"));
            err
        });
        assert_eq!(rendered, "dir/a.rs:1:1: first\ndir/a.rs:2:1: second");
    }

    #[test]
    fn a_call_site_span_points_past_the_last_character() {
        let source = "fn a() {}\nfn b() {}\n";
        let rendered = render_str(source, |_| syn::Error::new(Span::call_site(), "eof"));
        assert_eq!(rendered, "dir/a.rs:2:10: eof");
    }

    #[test]
    fn a_call_site_span_in_an_empty_file_is_line_one() {
        let err = syn::Error::new(Span::call_site(), "eof");
        assert_eq!(render(Path::new("e.rustx"), "", err), "e.rustx:1:1: eof");
    }
}
