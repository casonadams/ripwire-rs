use crate::domain::{IngestResult, Interner};
use serde_json::{Value, json};
use std::collections::HashMap;

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

pub(super) fn check_word_drift(
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
