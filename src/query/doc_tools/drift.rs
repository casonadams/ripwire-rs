use super::doc_scan::{is_dated_or_historical_doc, scan_doc_text_for_drift};
use super::presence::filter_present_in_sources_drift;
use crate::domain::{IngestResult, Interner};
use crate::query::SourceReader;
use crate::query::branch_tools::probe_name_history;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;

#[derive(Default)]
struct CorpusDocSummary {
    docs_scanned: usize,
    dated_docs: usize,
    prose_docs: usize,
    anchors: usize,
    checked: usize,
}

fn enrich_drift_item_history(root: &str, item: &mut Value, head_sha: &str) {
    let Some(sym) = item["referenced"].as_str().map(str::to_string) else {
        return;
    };
    let Some(hist) = probe_name_history(root, &sym, head_sha) else {
        return;
    };
    let fate = &hist["fate"];
    if let Some(v) = fate["v"].as_str() {
        item["fate"] = Value::String(v.to_string());
    }
    if let Some(c) = fate["commit"].as_str() {
        item["commit"] = Value::String(c.to_string());
    }
    if let Some(d) = fate["date"].as_str() {
        item["date"] = Value::String(d.to_string());
    }
    if let Some(s) = fate["subject"].as_str() {
        item["subject"] = Value::String(s.to_string());
    }
}

fn build_known_code_ident_set<'a>(
    ingest: &'a IngestResult,
    interner: &'a Interner,
) -> std::collections::HashSet<&'a str> {
    let mut sym_set: std::collections::HashSet<&str> = ingest
        .symbols
        .iter()
        .filter(|s| s.kind != crate::domain::SymKind::Section)
        .map(|s| interner.resolve(&s.name))
        .collect();
    for sc in ingest.symbols.iter().filter_map(|s| s.scope.as_ref()) {
        sym_set.insert(interner.resolve(sc));
    }
    for r in ingest
        .references
        .iter()
        .filter(|r| !r.lang.semantics().is_prose_document())
    {
        sym_set.insert(interner.resolve(&r.callee_name));
        if let Some(q) = &r.qualifier {
            sym_set.insert(interner.resolve(q));
        }
    }
    sym_set
}

fn scm_query_paths(root: &std::path::Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(root.join("src/languages")) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter_map(|entry| {
            entry
                .path()
                .join("tags.scm")
                .strip_prefix(root)
                .ok()
                .map(|path| path.to_string_lossy().into_owned())
        })
        .collect()
}

fn scan_corpus_markdown_docs(
    sources: &(impl SourceReader + ?Sized),
    ingest: &IngestResult,
    interner: &Interner,
    file_map: &HashMap<&str, u32>,
    sym_set: &std::collections::HashSet<&str>,
    drift_items: &mut Vec<Value>,
) -> CorpusDocSummary {
    let mut summary = CorpusDocSummary::default();
    for file in &ingest.files {
        let doc_path = interner.resolve(&file.path);
        if !file.lang.semantics().is_prose_document() {
            continue;
        }
        let Ok(bytes) = sources.read(doc_path) else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        summary.docs_scanned += 1;
        if is_dated_or_historical_doc(doc_path, text) {
            summary.dated_docs += 1;
        } else {
            let c = scan_doc_text_for_drift(
                text,
                doc_path,
                file_map,
                sym_set,
                ingest,
                interner,
                drift_items,
            );
            summary.prose_docs += usize::from(c.checked == 0);
            summary.anchors += c.total;
            summary.checked += c.checked;
        }
    }
    summary
}

pub fn doc_drift_with_sources_and_history(
    root: &str,
    sources: &(impl SourceReader + ?Sized),
    ingest: &IngestResult,
    interner: &Interner,
    with_history: bool,
) -> Value {
    let mut result = doc_drift_from_sources(
        root,
        sources,
        ingest,
        interner,
        &scm_query_paths(std::path::Path::new(root)),
    );
    if with_history {
        match crate::git::history::compute_git_at_stamp(root) {
            Ok(stamp) => {
                let head_sha = stamp.unwrap_or_default();
                if let Some(items) = result["drift"].as_array_mut() {
                    for item in items {
                        enrich_drift_item_history(root, item, &head_sha);
                    }
                }
            }
            Err(reason) => result["history"] = json!({ "unavailable": reason }),
        }
    }
    result
}

pub fn doc_drift_from_sources(
    root: &str,
    sources: &(impl SourceReader + ?Sized),
    ingest: &IngestResult,
    interner: &Interner,
    extra_paths: &[String],
) -> Value {
    let mut drift_items = Vec::new();
    let file_map: HashMap<&str, u32> = ingest
        .files
        .iter()
        .map(|f| (interner.resolve(&f.path), f.line_count))
        .collect();
    let sym_set = build_known_code_ident_set(ingest, interner);
    let summary = scan_corpus_markdown_docs(
        sources,
        ingest,
        interner,
        &file_map,
        &sym_set,
        &mut drift_items,
    );
    filter_present_in_sources_drift(sources, ingest, interner, extra_paths, &mut drift_items);
    let drifted_docs: std::collections::HashSet<&str> = drift_items
        .iter()
        .filter_map(|d| d["doc"].as_str())
        .collect();
    let clean_docs = summary
        .docs_scanned
        .saturating_sub(summary.dated_docs)
        .saturating_sub(drifted_docs.len());
    let unchecked = summary.anchors.saturating_sub(summary.checked);

    json!({
        "root": root,
        "docs_scanned": summary.docs_scanned,
        "clean_docs": clean_docs,
        "dated_docs": summary.dated_docs,
        "prose_docs": summary.prose_docs,
        "corpus": ingest.files.len(),
        "anchors": summary.anchors,
        "checked": summary.checked,
        "unchecked": unchecked,
        "drift_count": drift_items.len(),
        "drift": drift_items
    })
}
