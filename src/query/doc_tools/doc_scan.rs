use super::anchors::{
    DocAnchorCounts, collect_line_backtick_candidates, extract_line_resolved_symbols,
};
use super::line_drift::check_word_drift;
use crate::domain::{IngestResult, Interner};
use serde_json::{Value, json};
use std::collections::HashMap;

fn flush_corroborated_missing(
    doc_path: &str,
    resolving_lines: &[usize],
    pending_missing: Vec<(usize, &str, bool)>,
    drift_items: &mut Vec<Value>,
) {
    for (doc_line, sym, had_parens) in pending_missing {
        if had_parens || resolving_lines.iter().any(|&r| r.abs_diff(doc_line) <= 5) {
            drift_items.push(json!({
                "doc": doc_path,
                "line": doc_line,
                "doc_line": doc_line,
                "kind": "symbol",
                "referenced": sym,
                "why": "undefined"
            }));
        }
    }
}

pub(super) fn is_dated_or_historical_doc(doc_path: &str, text: &str) -> bool {
    let lower = doc_path.to_ascii_lowercase();
    if lower.ends_with("changelog.md") || lower.contains("/changelog") {
        return true;
    }
    text.lines().take(15).any(|l| {
        let bytes = l.as_bytes();
        bytes.windows(10).any(|w| {
            w[0..4].iter().all(u8::is_ascii_digit)
                && w[4] == b'-'
                && w[5..7].iter().all(u8::is_ascii_digit)
                && w[7] == b'-'
                && w[8..10].iter().all(u8::is_ascii_digit)
        })
    })
}

pub fn scan_doc_text_for_drift(
    text: &str,
    doc_path: &str,
    file_map: &HashMap<&str, u32>,
    sym_set: &std::collections::HashSet<&str>,
    ingest: &IngestResult,
    interner: &Interner,
    drift_items: &mut Vec<Value>,
) -> DocAnchorCounts {
    let mut in_code_fence = false;
    let mut resolving_lines = Vec::new();
    let mut pending_missing = Vec::new();
    let mut counts = DocAnchorCounts::default();
    for (line_idx, line) in text.lines().enumerate() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("```") || trimmed.starts_with("~~~") {
            in_code_fence = !in_code_fence;
            continue;
        }
        if in_code_fence {
            continue;
        }
        let doc_line = line_idx + 1;
        let line_syms = extract_line_resolved_symbols(line, sym_set);
        for word in line.split_whitespace() {
            if let Some(item) = check_word_drift(
                word, doc_path, doc_line, file_map, &line_syms, ingest, interner,
            ) {
                counts.total += 1;
                counts.checked += 1;
                drift_items.push(item);
            }
        }
        let line_counts = collect_line_backtick_candidates(
            line,
            doc_line,
            sym_set,
            &mut resolving_lines,
            &mut pending_missing,
        );
        counts.total += line_counts.total;
        counts.checked += line_counts.checked;
    }
    flush_corroborated_missing(doc_path, &resolving_lines, pending_missing, drift_items);
    counts
}
