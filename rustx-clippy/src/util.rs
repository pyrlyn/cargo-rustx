//! Source helpers shared by the early and the late lints.

use rustc_span::source_map::SourceMap;
use rustc_span::{BytePos, Span};

/// `span` grown to the whole lines it sits on, newline included, when nothing else shares those
/// lines; otherwise `span` itself. Removing the result leaves no blank line behind.
pub fn whole_lines(sm: &SourceMap, span: Span) -> Span {
    let lo = sm.lookup_byte_offset(span.lo());
    let hi = sm.lookup_byte_offset(span.hi());
    let Some(src) = lo.sf.src.as_deref() else {
        return span;
    };
    let (start, end) = (lo.pos.to_usize(), hi.pos.to_usize());
    let (Some(before), Some(after)) = (src.get(..start), src.get(end..)) else {
        return span;
    };
    let line_start = before.rfind('\n').map_or(0, |at| at + 1);
    let Some(newline) = after.find('\n') else {
        return span;
    };
    if before[line_start..].trim().is_empty() && after[..newline].trim().is_empty() {
        let back = u32::try_from(start - line_start).unwrap_or(0);
        let forward = u32::try_from(newline + 1).unwrap_or(0);
        span.with_lo(span.lo() - BytePos(back))
            .with_hi(span.hi() + BytePos(forward))
    } else {
        span
    }
}

/// The indentation of the line `span` starts on.
pub fn indent(sm: &SourceMap, span: Span) -> String {
    let lo = sm.lookup_byte_offset(span.lo());
    let Some(src) = lo.sf.src.as_deref() else {
        return String::new();
    };
    let start = lo.pos.to_usize();
    let line_start = src.get(..start).and_then(|b| b.rfind('\n')).map_or(0, |at| at + 1);
    src.get(line_start..start)
        .map(|prefix| prefix.chars().take_while(|c| c.is_whitespace()).collect())
        .unwrap_or_default()
}
