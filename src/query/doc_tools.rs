pub use super::branch_tools::*;
use crate::domain::{IngestResult, Interner};
use crate::languages::common::flags::FlagFact;
use crate::query::{FileSources, SourceReader};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::fs;

pub fn execute_mentions_query<W: std::io::Write>(
    root: &str,
    ingest: &IngestResult,
    interner: &Interner,
    symbol: &str,
    out: &mut W,
) -> std::io::Result<()> {
    if crate::navigate::find_symbols_by_name(symbol, ingest, interner).is_empty() {
        return Err(std::io::Error::other(format!(
            "ripwire-rs: --mentions symbol not found: {symbol}"
        )));
    }
    let report = call_mentions_in_root(root, ingest, interner, symbol);
    crate::serialize::serialize_mentions_xml(&report, out)
}

pub fn call_mentions_in_root(
    root: &str,
    ingest: &IngestResult,
    interner: &Interner,
    symbol_name: &str,
) -> Value {
    mentions_from_sources(
        &FileSources {
            root: std::path::Path::new(root),
        },
        ingest,
        interner,
        symbol_name,
    )
}

pub fn mentions_from_sources(
    sources: &(impl SourceReader + ?Sized),
    ingest: &IngestResult,
    interner: &Interner,
    symbol_name: &str,
) -> Value {
    let mut matching_docs = Vec::new();
    let backtick_target = format!("`{symbol_name}`");

    for file in &ingest.files {
        let path = interner.resolve(&file.path);
        if file.lang.semantics().is_prose_document()
            && let Ok(bytes) = sources.read(path)
            && let Ok(content) = std::str::from_utf8(&bytes)
        {
            let occurrences = content.matches(&backtick_target).count();
            if occurrences > 0 {
                matching_docs.push(json!({
                    "file": path,
                    "mentions": occurrences
                }));
            }
        }
    }

    json!({
        "symbol": symbol_name,
        "docs": matching_docs,
        "total_mentions": matching_docs.iter().map(|d| d["mentions"].as_u64().unwrap_or(0)).sum::<u64>()
    })
}

/// Execute `doc_drift` tool: verify checkable doc references.
fn extract_line_resolved_symbols<'a>(
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

fn check_line_moved_drift(
    doc_path: &str,
    doc_line: usize,
    target_file: &str,
    target_line: u32,
    line_syms: &[&str],
    ingest: &IngestResult,
    interner: &Interner,
) -> Option<Value> {
    use crate::domain::CorpusView;
    if line_syms.is_empty() || ingest.symbols.is_empty() {
        return None;
    }
    let view = (ingest, interner);
    let got_sym = crate::navigate::resolve_symbol_at(target_file, target_line, ingest, interner);
    for &sym_name in line_syms {
        if got_sym.is_some_and(|gs| view.sym_name(gs) == sym_name) {
            return None;
        }
        let Some(moved) = ingest
            .symbols
            .iter()
            .find(|s| view.sym_name(s) == sym_name && view.sym_path(s) == target_file)
        else {
            continue;
        };
        if target_line < moved.line || target_line > moved.end_line() {
            let got_name = got_sym.map_or("", |gs| view.sym_name(gs));
            return Some(json!({
                "doc": doc_path,
                "line": doc_line,
                "doc_line": doc_line,
                "kind": "file-line",
                "referenced": format!("{target_file}:{target_line}"),
                "target_file": target_file,
                "claimed_line": target_line,
                "sym": sym_name,
                "got": got_name,
                "tgt": format!("{target_file}:{}", moved.line),
                "why": "line-moved"
            }));
        }
    }
    None
}

fn check_word_drift(
    word: &str,
    doc_path: &str,
    doc_line: usize,
    file_map: &HashMap<&str, u32>,
    line_syms: &[&str],
    ingest: &IngestResult,
    interner: &Interner,
) -> Option<Value> {
    let clean = word.trim_matches(|c: char| matches!(c, '`' | '(' | ')' | '[' | ']' | ',' | ';'));
    let (target_file, line_str) = clean.split_once(':')?;
    let target_line = line_str
        .trim_matches(|c: char| !c.is_ascii_digit())
        .parse::<u32>()
        .ok()?;
    let &total_lines = file_map.get(target_file)?;
    if target_line > total_lines {
        Some(json!({
            "doc": doc_path,
            "line": doc_line,
            "doc_line": doc_line,
            "kind": "file-line",
            "referenced": format!("{target_file}:{target_line}"),
            "target_file": target_file,
            "claimed_line": target_line,
            "actual_lines": total_lines,
            "why": "line-out-of-bounds"
        }))
    } else {
        check_line_moved_drift(
            doc_path,
            doc_line,
            target_file,
            target_line,
            line_syms,
            ingest,
            interner,
        )
    }
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

fn is_language_builtin_ident(sym: &str) -> bool {
    matches!(
        sym,
        "AbortController"
            | "AggregateError"
            | "ArrayBuffer"
            | "AsyncFunction"
            | "BigInt64Array"
            | "BigUint64Array"
            | "DataView"
            | "EvalError"
            | "FinalizationRegistry"
            | "Float32Array"
            | "Float64Array"
            | "Int16Array"
            | "Int32Array"
            | "Int8Array"
            | "RangeError"
            | "ReferenceError"
            | "SharedArrayBuffer"
            | "SyntaxError"
            | "TextDecoder"
            | "TextEncoder"
            | "TypeError"
            | "URIError"
            | "URLSearchParams"
            | "Uint16Array"
            | "Uint32Array"
            | "Uint8Array"
            | "Uint8ClampedArray"
            | "WeakMap"
            | "WeakRef"
            | "WeakSet"
            | "clearInterval"
            | "clearTimeout"
            | "decodeURIComponent"
            | "encodeURIComponent"
            | "globalThis"
            | "queueMicrotask"
            | "setInterval"
            | "setTimeout"
            | "structuredClone"
            | "ValueError"
            | "RuntimeError"
            | "KeyError"
            | "IndexError"
            | "OSError"
            | "AttributeError"
            | "NotImplementedError"
            | "FileNotFoundError"
            | "PermissionError"
            | "TimeoutError"
            | "ConnectionError"
            | "ImportError"
            | "AssertionError"
    )
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

fn collect_line_backtick_candidates<'a>(
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

fn is_dated_or_historical_doc(doc_path: &str, text: &str) -> bool {
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

pub fn enrich_drift_item_history(root: &str, item: &mut Value, head_sha: &str) {
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

pub fn filter_present_in_code_drift(
    root_path: &std::path::Path,
    ingest: &IngestResult,
    interner: &Interner,
    drift_items: &mut Vec<Value>,
) {
    filter_present_in_sources_drift(
        &FileSources { root: root_path },
        ingest,
        interner,
        &scm_query_paths(root_path),
        drift_items,
    );
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

#[derive(Default)]
struct CorpusDocSummary {
    docs_scanned: usize,
    dated_docs: usize,
    prose_docs: usize,
    anchors: usize,
    checked: usize,
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

pub fn call_doc_drift_with_history(
    root: &str,
    ingest: &IngestResult,
    interner: &Interner,
    with_history: bool,
) -> Value {
    doc_drift_with_sources_and_history(
        root,
        &FileSources {
            root: std::path::Path::new(root),
        },
        ingest,
        interner,
        with_history,
    )
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

#[derive(Default)]
struct GateEntry {
    name: String,
    kind: &'static str,
    default_val: String,
    is_dark: bool,
    file: String,
    line: usize,
    regions: usize,
    loc: usize,
    reads: Vec<(String, usize)>,
}

impl GateEntry {
    fn configure(&mut self, def_val: String, is_dark: bool) {
        if self.reads.len() == 1 {
            self.default_val = def_val;
            self.is_dark = is_dark;
        }
    }
}

struct SettingsGate {
    name: String,
    default: String,
    dark: bool,
    define_line: usize,
}

#[derive(Default)]
struct FlagsScanTally {
    by_key: std::collections::BTreeMap<(String, &'static str), GateEntry>,
    settings: Vec<(SettingsGate, String)>,
    conditional: std::collections::HashMap<String, Vec<(String, usize, usize)>>,
}

impl FlagsScanTally {
    fn record_site(
        &mut self,
        kind: &'static str,
        name: String,
        path: &str,
        line: usize,
    ) -> &mut GateEntry {
        let entry = self
            .by_key
            .entry((name.clone(), kind))
            .or_insert_with(|| GateEntry {
                name,
                kind,
                default_val: "unset".to_string(),
                is_dark: true,
                file: path.to_string(),
                line,
                regions: 0,
                loc: 0,
                reads: Vec::new(),
            });
        entry.reads.push((path.to_string(), line));
        entry
    }

    /// A settings gate counts only where some `#if`-family condition uses it; those uses are its reads.
    fn resolve_settings_gates(&mut self) {
        for (gate, path) in std::mem::take(&mut self.settings) {
            let key = (gate.name.clone(), "compile");
            let Some(uses) = self.conditional.get(&gate.name) else {
                continue;
            };
            if self.by_key.contains_key(&key) {
                continue;
            }
            let mut reads: Vec<(String, usize)> =
                uses.iter().map(|(p, l, _)| (p.clone(), *l)).collect();
            reads.dedup();
            self.by_key.insert(
                key,
                GateEntry {
                    name: gate.name,
                    kind: "compile",
                    default_val: gate.default,
                    is_dark: gate.dark,
                    file: path,
                    line: gate.define_line + 1,
                    regions: uses.len(),
                    loc: uses.iter().map(|(_, _, loc)| loc).sum(),
                    reads,
                },
            );
        }
    }
}

fn scan_file_flags(
    sources: &(impl SourceReader + ?Sized),
    path: &str,
    lang: crate::domain::Lang,
    tally: &mut FlagsScanTally,
) {
    let Ok(bytes) = sources.read(path) else {
        return;
    };
    let Ok(text) = std::str::from_utf8(&bytes) else {
        return;
    };
    let lines: Vec<&str> = text.lines().collect();
    let literal = literal_block_lines(&lines, lang);
    for fact in lang.semantics().flag_facts(&lines, path) {
        match fact {
            FlagFact::Conditional { name, line, loc } => {
                tally
                    .conditional
                    .entry(name)
                    .or_default()
                    .push((path.to_string(), line + 1, loc));
            }
            FlagFact::Env { line, .. }
            | FlagFact::Option { line, .. }
            | FlagFact::Setting { line, .. }
                if literal[line] => {}
            FlagFact::Env { name, line } => {
                tally.record_site("env", name, path, line + 1);
            }
            FlagFact::Option {
                kind,
                name,
                default,
                dark,
                line,
            } => tally
                .record_site(kind, name, path, line + 1)
                .configure(default, dark),
            FlagFact::Setting {
                name,
                default,
                dark,
                define_line,
                ..
            } => tally.settings.push((
                SettingsGate {
                    name,
                    default,
                    dark,
                    define_line,
                },
                path.to_string(),
            )),
        }
    }
}

fn literal_block_lines(lines: &[&str], lang: crate::domain::Lang) -> Vec<bool> {
    let mut literal = vec![false; lines.len()];
    let mut until: Option<&str> = None;
    for (idx, line) in lines.iter().enumerate() {
        match until {
            Some(term) => {
                literal[idx] = true;
                if line.trim() == term {
                    until = None;
                }
            }
            None => until = lang.semantics().quoted_heredoc_terminator(line),
        }
    }
    literal
}

fn scan_flag_manifests(sources: &(impl SourceReader + ?Sized), tally: &mut FlagsScanTally) {
    for lang in crate::languages::ALL_LANGS {
        for manifest in lang.semantics().flag_manifests() {
            scan_file_flags(sources, manifest, lang, tally);
        }
    }
}

pub fn call_flags_in_root(root: &str, ingest: &IngestResult, interner: &Interner) -> Value {
    flags_from_sources(
        root,
        &FileSources {
            root: std::path::Path::new(root),
        },
        ingest,
        interner,
    )
}

pub fn flags_from_sources(
    root: &str,
    sources: &(impl SourceReader + ?Sized),
    ingest: &IngestResult,
    interner: &Interner,
) -> Value {
    let mut tally = FlagsScanTally::default();

    for file in &ingest.files {
        if !file.lang.semantics().is_prose_document() {
            let path = interner.resolve(&file.path);
            scan_file_flags(sources, path, file.lang, &mut tally);
        }
    }
    scan_flag_manifests(sources, &mut tally);
    tally.resolve_settings_gates();

    let mut flags = Vec::with_capacity(tally.by_key.len());
    let mut compile_count = 0usize;
    let mut cmake_count = 0usize;
    let mut env_count = 0usize;
    let mut dark_gates = 0usize;
    for entry in tally.by_key.into_values() {
        match entry.kind {
            "compile" => compile_count += 1,
            "cmake" => cmake_count += 1,
            _ => env_count += 1,
        }
        dark_gates += usize::from(entry.is_dark);
        let reads_json: Vec<Value> = entry
            .reads
            .iter()
            .map(|(p, l)| json!({ "path": p, "line": l }))
            .collect();
        flags.push(json!({
            "name": entry.name,
            "flag": entry.name,
            "kind": entry.kind,
            "default": entry.default_val,
            "dark": usize::from(entry.is_dark),
            "regions": entry.regions,
            "loc": entry.loc,
            "reads_count": entry.reads.len(),
            "reads": reads_json,
            "file": entry.file,
            "path": entry.file,
            "line": entry.line
        }));
    }

    json!({
        "root": root,
        "files": ingest.files.len(),
        "flags_count": flags.len(),
        "compile_count": compile_count,
        "cmake_count": cmake_count,
        "env_count": env_count,
        "dark_gates": dark_gates,
        "gates": flags.clone(),
        "flags": flags
    })
}
