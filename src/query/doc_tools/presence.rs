use crate::domain::{IngestResult, Interner};
use crate::query::SourceReader;
use serde_json::Value;

pub fn collect_file_wanted_tokens(
    bytes: &[u8],
    wants_first: &[bool; 256],
    wanted: &std::collections::HashSet<String>,
) -> Vec<String> {
    let mut found = Vec::new();
    for tok in bytes.split(|&b| !b.is_ascii_alphanumeric() && b != b'_') {
        if tok.len() >= 4
            && wants_first[tok[0] as usize]
            && let Ok(s) = std::str::from_utf8(tok)
            && wanted.contains(s)
        {
            found.push(s.to_string());
        }
    }
    found
}

pub fn filter_present_in_sources_drift(
    sources: &(impl SourceReader + ?Sized),
    ingest: &IngestResult,
    interner: &Interner,
    extra_paths: &[String],
    drift_items: &mut Vec<Value>,
) {
    use rayon::prelude::*;
    let wanted: std::collections::HashSet<String> = drift_items
        .iter()
        .filter(|d| d["kind"].as_str() == Some("symbol"))
        .filter_map(|d| d["referenced"].as_str().map(str::to_string))
        .collect();
    if wanted.is_empty() {
        return;
    }
    let mut wants_first = [false; 256];
    for w in &wanted {
        if let Some(&b) = w.as_bytes().first() {
            wants_first[b as usize] = true;
        }
    }
    let mut found_in_code: std::collections::HashSet<String> = ingest
        .files
        .par_iter()
        .filter(|f| !f.lang.semantics().is_prose_document())
        .flat_map_iter(|f| {
            let rel = interner.resolve(&f.path);
            sources
                .read(rel)
                .map(|bytes| collect_file_wanted_tokens(&bytes, &wants_first, &wanted))
                .unwrap_or_default()
        })
        .collect();
    for path in extra_paths {
        if let Ok(bytes) = sources.read(path) {
            found_in_code.extend(collect_file_wanted_tokens(&bytes, &wants_first, &wanted));
        }
    }
    drift_items.retain(|d| {
        if d["kind"].as_str() == Some("symbol")
            && let Some(sym) = d["referenced"].as_str()
        {
            !found_in_code.contains(sym)
        } else {
            true
        }
    });
}
