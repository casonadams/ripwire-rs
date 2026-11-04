use super::terms::contains_term_aware;
use crate::domain::{IngestResult, Interner, NodeId, SymKind};
use crate::graph::Graph;
use crate::query::search::call_lego_with;
use serde_json::{Value, json};

/// Most direct callers a name-exact `--for` lists; the rest stay behind `--callers`.
const FOR_CALLERS_CAP: usize = 8;

/// The direct callers of the one definition a name-exact `--for` answers: `of`, the `total`, and up to
/// `FOR_CALLERS_CAP` rows ranked by importance. `None` when the name binds several definitions, the graph
/// is absent, or nothing calls it (`in=` on the signature row already says so).
pub(super) fn collect_for_callers(
    ingest: &IngestResult,
    interner: &Interner,
    graph: Option<&crate::graph::Graph>,
    window_rows: &[(f64, &crate::domain::Symbol)],
    term: &str,
    ranks: &[f64],
) -> Option<Value> {
    let [(_, sym)] = window_rows else {
        return None;
    };
    if interner.resolve(&sym.name) != term {
        return None;
    }
    let graph = graph?;
    let mut callers: Vec<(NodeId, bool)> = graph
        .callers(sym.id)
        .map(|(caller, _)| {
            let by_name = graph
                .edge_prov(caller, sym.id)
                .is_some_and(crate::resolve::EdgeProv::is_by_name);
            (caller, by_name)
        })
        .collect();
    if callers.is_empty() {
        return None;
    }
    let rank_of = |id: NodeId| ranks.get(id as usize).copied().unwrap_or(0.0);
    callers.sort_by(|a, b| {
        rank_of(b.0)
            .total_cmp(&rank_of(a.0))
            .then_with(|| a.0.cmp(&b.0))
    });
    let total = callers.len();
    let rows: Vec<Value> = callers
        .into_iter()
        .take(FOR_CALLERS_CAP)
        .filter_map(|(id, by_name)| {
            let caller = ingest.sym(id)?;
            Some(json!({
                "name": interner.resolve(&caller.name),
                "kind": caller.kind.tag(),
                "path": ingest.sym_file_path(caller, interner),
                "line": caller.line,
                "by_name": by_name,
            }))
        })
        .collect();
    Some(json!({ "of": term, "total": total, "rows": rows }))
}

/// Interfaces that are ranked, or declared in a file holding a ranked symbol, and have implementors.
pub(super) fn collect_for_lego(
    ingest: &IngestResult,
    interner: &Interner,
    ranked: &[(f64, &crate::domain::Symbol)],
) -> Vec<Value> {
    let ranked_files: std::collections::HashSet<u32> =
        ranked.iter().map(|(_, s)| s.file_id).collect();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let index = std::cell::OnceCell::new();
    for sym in &ingest.symbols {
        if sym.kind != SymKind::Interface || !ranked_files.contains(&sym.file_id) {
            continue;
        }
        let name = interner.resolve(&sym.name);
        if !seen.insert(name) {
            continue;
        }
        let lego = call_lego_with(ingest, interner, name, &index);
        if lego["count"].as_u64().unwrap_or(0) == 0 {
            continue;
        }
        let path = ingest
            .files
            .get(sym.file_id as usize)
            .map_or("", |f| interner.resolve(&f.path));
        out.push(json!({
            "name": name,
            "path": path,
            "line": sym.line,
            "implementors": lego["implementors"]
        }));
    }
    out
}

pub(super) fn collect_for_compose(
    ingest: &IngestResult,
    interner: &Interner,
    ranked: &[(f64, &crate::domain::Symbol)],
) -> Vec<Value> {
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let ranked_files: std::collections::HashSet<u32> =
        ranked.iter().map(|(_, s)| s.file_id).collect();
    let mut out = Vec::new();
    for sym in &ingest.symbols {
        if sym.kind != SymKind::Field || !ranked_files.contains(&sym.file_id) {
            continue;
        }
        let Some(owner) = view.sym_scope(sym) else {
            continue;
        };
        out.push(json!({
            "name": view.sym_name(sym),
            "owner": owner,
            "rel": "uses",
        }));
        if out.len() >= 32 {
            break;
        }
    }
    out
}

pub(super) fn collect_for_hops(
    ingest: &IngestResult,
    interner: &Interner,
    graph_opt: Option<&Graph>,
    ranked: &[(f64, &crate::domain::Symbol)],
    page: crate::serialize::PageRequest,
) -> Vec<Value> {
    use crate::domain::CorpusView;
    let Some(graph) = graph_opt else {
        return Vec::new();
    };
    let view = (ingest, interner);
    let mut hops = Vec::new();
    for &(_, sym) in ranked {
        let callees: Vec<Value> = graph
            .callees(sym.id)
            .filter_map(|(cid, _)| ingest.sym(cid))
            .map(|csym| {
                json!({
                    "name": view.sym_name(csym),
                    "line": csym.line,
                })
            })
            .collect();
        if callees.is_empty() {
            continue;
        }
        let calls = crate::serialize::Paged::new(&callees, page, |c| c.to_string().len())
            .rows()
            .to_vec();
        hops.push(json!({
            "name": view.sym_name(sym),
            "path": view.sym_path(sym),
            "line": sym.line,
            "total": callees.len(),
            "calls": calls,
        }));
    }
    let window = crate::serialize::Paged::new(&hops, page, |h| h.to_string().len()).window;
    hops.into_iter()
        .skip(window.begin)
        .take(window.shown)
        .collect()
}

pub(super) fn collect_for_tree_rows(
    ingest: &IngestResult,
    interner: &Interner,
    scored: &[(f64, &crate::domain::Symbol)],
    begin: usize,
    end: usize,
) -> Vec<Value> {
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let shown_files: std::collections::HashSet<u32> =
        scored[begin..end].iter().map(|(_, s)| s.file_id).collect();
    let mut seen_tree_files = std::collections::HashSet::new();
    let mut rows = Vec::new();
    for &(_, sym) in scored.iter().skip(end) {
        if rows.len() >= 12 {
            break;
        }
        if !shown_files.contains(&sym.file_id) && seen_tree_files.insert(sym.file_id) {
            rows.push(json!({
                "path": view.sym_path(sym),
                "sym": view.sym_name(sym),
                "kind": sym.kind.tag(),
            }));
        }
    }
    rows
}

pub(super) fn collect_for_doc_mentions(
    ingest: &IngestResult,
    interner: &Interner,
    raw_terms: &[&str],
    page: crate::serialize::PageRequest,
) -> Vec<Value> {
    let mut doc_hits: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for sym in &ingest.symbols {
        if sym.kind != SymKind::Section || !sym.lang.semantics().is_prose_document() {
            continue;
        }
        let sec_lower = interner.resolve(&sym.name).to_ascii_lowercase();
        let hits = raw_terms
            .iter()
            .filter(|&&t| {
                let low = t.to_ascii_lowercase();
                contains_term_aware(&sec_lower, &low)
            })
            .count();
        if hits > 0 {
            let p = ingest.sym_file_path(sym, interner);
            *doc_hits.entry(p).or_default() += hits;
        }
    }
    let mut docs: Vec<(&str, usize)> = doc_hits.into_iter().collect();
    docs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let window =
        crate::serialize::Paged::new(&docs, page, |d| crate::serialize::text_bytes(&[d.0])).window;
    docs[window.begin..window.end]
        .iter()
        .map(|&(p, m)| json!({ "path": p, "mentions": m }))
        .collect()
}
