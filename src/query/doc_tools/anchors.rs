use super::builtins::is_language_builtin_ident;

pub(super) fn extract_line_resolved_symbols<'a>(
    line: &'a str,
    sym_set: &std::collections::HashSet<&str>,
) -> Vec<&'a str> {
    let mut out = Vec::new();
    let mut rest = line;
    while let Some(start) = rest.find('`') {
        let after_open = &rest[start + 1..];
        let Some(end) = after_open.find('`') else {
            break;
        };
        if let Some(sym) = is_candidate_symbol_anchor(&after_open[..end])
            && sym_set.contains(sym)
        {
            out.push(sym);
        }
        rest = &after_open[end + 1..];
    }
    out
}

fn is_code_shaped_ident(sym: &str) -> bool {
    let bytes = sym.as_bytes();
    let has_underscore = bytes.contains(&b'_');
    let camel_seam = bytes
        .windows(2)
        .any(|w| w[0].is_ascii_lowercase() && w[1].is_ascii_uppercase());
    let k_const = bytes.len() > 1 && bytes[0] == b'k' && bytes[1].is_ascii_uppercase();
    has_underscore || camel_seam || k_const
}

fn is_candidate_symbol_anchor(span: &str) -> Option<&str> {
    let trimmed = span.trim();
    let had_parens = trimmed.ends_with("()");
    let raw = trimmed.strip_suffix("()").unwrap_or(trimmed);
    if raw.contains('/')
        || raw.contains(' ')
        || raw.contains('=')
        || raw.contains('-')
        || raw.starts_with('@')
    {
        return None;
    }
    let sym = raw
        .rsplit("::")
        .next()
        .unwrap_or(raw)
        .rsplit('.')
        .next()
        .unwrap_or(raw);
    let valid_ident = sym.len() >= 4
        && !is_language_builtin_ident(sym)
        && sym
            .bytes()
            .next()
            .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && sym.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
    if valid_ident && (had_parens || is_code_shaped_ident(sym)) {
        Some(sym)
    } else {
        None
    }
}

#[derive(Default, Clone, Copy)]
pub struct DocAnchorCounts {
    pub total: usize,
    pub checked: usize,
}

fn is_counted_backtick_anchor(span: &str) -> bool {
    let t = span.trim();
    !t.is_empty()
        && !t.starts_with('-')
        && !t.starts_with('@')
        && !t
            .bytes()
            .any(|b| matches!(b, b' ' | b'\t' | b'<' | b'>' | b'='))
        && t.bytes().any(|b| b.is_ascii_alphabetic())
}

fn record_backtick_span<'a>(
    span: &'a str,
    doc_line: usize,
    sym_set: &std::collections::HashSet<&str>,
    counts: &mut DocAnchorCounts,
    resolving_lines: &mut Vec<usize>,
    pending_missing: &mut Vec<(usize, &'a str, bool)>,
) {
    if !is_counted_backtick_anchor(span) {
        return;
    }
    counts.total += 1;
    let Some(sym) = is_candidate_symbol_anchor(span) else {
        return;
    };
    counts.checked += 1;
    if !sym_set.contains(sym) {
        pending_missing.push((doc_line, sym, span.trim().ends_with("()")));
    } else if resolving_lines.last().copied() != Some(doc_line) {
        resolving_lines.push(doc_line);
    }
}

pub(super) fn collect_line_backtick_candidates<'a>(
    line: &'a str,
    doc_line: usize,
    sym_set: &std::collections::HashSet<&str>,
    resolving_lines: &mut Vec<usize>,
    pending_missing: &mut Vec<(usize, &'a str, bool)>,
) -> DocAnchorCounts {
    let mut counts = DocAnchorCounts::default();
    if sym_set.is_empty() {
        return counts;
    }
    let mut rest = line;
    while let Some(start) = rest.find('`') {
        let after_open = &rest[start + 1..];
        let Some(end) = after_open.find('`') else {
            break;
        };
        record_backtick_span(
            &after_open[..end],
            doc_line,
            sym_set,
            &mut counts,
            resolving_lines,
            pending_missing,
        );
        rest = &after_open[end + 1..];
    }
    counts
}
