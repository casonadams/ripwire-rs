use super::ForTaskOptions;
use crate::domain::{CorpusView, IngestResult, Interner, Symbol};
use crate::graph::Graph;
use serde_json::{Value, json};

/// Most rows the default body posture serves inline (the exact-name definitions, in rank order).
const AUTO_BODY_ROWS: usize = 6;

fn read_file_cached<'a, 's>(
    sources: &'s dyn crate::query::SourceReader,
    rel_path: &str,
    file_id: u32,
    cache: &'a mut std::collections::HashMap<u32, std::borrow::Cow<'s, [u8]>>,
) -> &'a [u8] {
    cache
        .entry(file_id)
        .or_insert_with(|| sources.read(rel_path).unwrap_or_default())
}

/// Longest signature a ranked row shows. A constant's signature is its whole initializer, which would
/// otherwise take the budget of a dozen rows; `--expand` serves the rest.
const MAX_SIGNATURE_BYTES: usize = 240;

fn cap_signature(mut sig: String) -> String {
    if sig.len() > MAX_SIGNATURE_BYTES {
        let mut cut = MAX_SIGNATURE_BYTES;
        while !sig.is_char_boundary(cut) {
            cut -= 1;
        }
        sig.truncate(cut);
        sig.push('…');
    }
    sig
}

/// Bytes one ranked symbol row costs: its name, file path and signature span.
pub(super) fn symbol_row_bytes(
    sym: &crate::domain::Symbol,
    ingest: &IngestResult,
    interner: &Interner,
) -> usize {
    let signature =
        (sym.sig_end_byte.saturating_sub(sym.sig_start_byte) as usize).min(MAX_SIGNATURE_BYTES);
    crate::serialize::text_bytes(&[
        interner.resolve(&sym.name),
        ingest.sym_file_path(sym, interner),
    ]) + signature
}

/// Rows that the body policy asks to serve inline. Explicit `--detail` and `--auto-bodies` take the first N
/// rows. The default posture serves only the exact-name definitions on the name-exact route.
pub(super) fn body_wanted(
    idx: usize,
    name: &str,
    opts: ForTaskOptions<'_>,
    exact_term: Option<&str>,
) -> bool {
    if opts.signatures_only {
        return false;
    }
    match opts.detail.or(opts.auto_bodies.then_some(AUTO_BODY_ROWS)) {
        Some(count) => idx < count,
        None => {
            idx < AUTO_BODY_ROWS && exact_term.is_some_and(|term| term.eq_ignore_ascii_case(name))
        }
    }
}

/// One ranked row. `wants_body` is the body policy's ask; the budget decides whether the body is served.
struct RankedRow<'a> {
    score: f64,
    sym: &'a crate::domain::Symbol,
    is_tested: bool,
    wants_body: bool,
}

fn build_for_symbol_entry<'s>(
    row: &RankedRow<'_>,
    view: &dyn crate::domain::CorpusView<'_>,
    sources: Option<&'s dyn crate::query::SourceReader>,
    graph: Option<&Graph>,
    file_cache: &mut std::collections::HashMap<u32, std::borrow::Cow<'s, [u8]>>,
    body_room: &mut usize,
) -> Value {
    let sym = row.sym;
    let name = view.sym_name(sym);
    let path = view.sym_path(sym);
    let bytes = sources
        .map(|sources| read_file_cached(sources, path, sym.file_id, file_cache))
        .unwrap_or_default();
    let sig = cap_signature(sym.extract_signature(bytes, name));
    let in_deg = graph.map_or(0, |g| g.in_degree(sym.id));
    let end_line = sym.line.saturating_add(sym.loc.saturating_sub(1));
    let mut obj = json!({
        "name": name,
        "kind": sym.kind.tag(),
        "path": path,
        "line": sym.line,
        "end_line": end_line,
        "sig": sig,
        "cx": sym.cx,
        "ccx": sym.ccx,
        "in": in_deg,
        "amp": in_deg.max(1),
        "tested": u8::from(row.is_tested),
        "score": (row.score * 100.0).round() / 100.0
    });
    if row.wants_body
        && let Some(body) = sym.source_text(bytes)
        && body.len() <= *body_room
    {
        *body_room -= body.len();
        obj["body"] = Value::String(body.into_owned());
    }
    obj
}

/// The bundle kind, why bodies were left out (if they were), and an executable `--expand` for the first row without one.
pub(super) fn body_disclosure(
    opts: ForTaskOptions<'_>,
    conceptual: bool,
    rows: &[Value],
    wanted: usize,
    served: usize,
) -> (&'static str, Option<&'static str>, Option<String>) {
    let bundle = if conceptual { "compact" } else { "auto" };
    let reason = if rows.is_empty() {
        None
    } else if opts.signatures_only {
        Some("signatures-only")
    } else if served < wanted {
        Some("budget")
    } else if served == 0 && conceptual && opts.detail.is_none() && !opts.auto_bodies {
        Some("compact-route")
    } else {
        None
    };
    let follow = reason
        .and(rows.iter().find(|row| !row["body"].is_string()))
        .map(|row| {
            format!(
                "--expand={}:{}",
                row["path"].as_str().unwrap_or_default(),
                row["name"].as_str().unwrap_or_default()
            )
        });
    (bundle, reason, follow)
}

pub(super) struct RowsRequest<'a> {
    pub(super) ingest: &'a IngestResult,
    pub(super) interner: &'a Interner,
    pub(super) opts: ForTaskOptions<'a>,
    pub(super) exact_term: Option<&'a str>,
    pub(super) budget: Option<usize>,
}

pub(super) fn build_symbol_rows(
    req: &RowsRequest<'_>,
    begin: usize,
    window: &[(f64, &Symbol)],
) -> (Vec<Value>, usize) {
    let view = (req.ingest, req.interner);
    let wants: Vec<bool> = window
        .iter()
        .enumerate()
        .map(|(idx, (_, sym))| {
            body_wanted(begin + idx, view.sym_name(sym), req.opts, req.exact_term)
        })
        .collect();
    let wanted = wants.iter().filter(|w| **w).count();
    let mut body_room = body_room(req, window);
    let mut file_cache = std::collections::HashMap::new();
    let tested_bitmap = req.opts.graph.map(|g| view.compute_test_reachability(g));
    let rows = window
        .iter()
        .zip(&wants)
        .map(|((score, sym), &wants_body)| {
            let is_tested = tested_bitmap
                .as_ref()
                .and_then(|bm| bm.get(sym.id as usize).copied())
                .unwrap_or_else(|| view.is_test_symbol(sym));
            let row = RankedRow {
                score: *score,
                sym,
                is_tested,
                wants_body,
            };
            build_for_symbol_entry(
                &row,
                &view,
                req.opts.sources,
                req.opts.graph,
                &mut file_cache,
                &mut body_room,
            )
        })
        .collect();
    (rows, wanted)
}

fn body_room(req: &RowsRequest<'_>, window: &[(f64, &Symbol)]) -> usize {
    if req.opts.detail.is_some() || req.opts.auto_bodies {
        return usize::MAX;
    }
    let shown_bytes: usize = window
        .iter()
        .map(|(_, sym)| symbol_row_bytes(sym, req.ingest, req.interner))
        .sum();
    req.budget
        .unwrap_or(crate::serialize::DEFAULT_PACK_BUDGET_BYTES)
        .saturating_sub(shown_bytes)
}
