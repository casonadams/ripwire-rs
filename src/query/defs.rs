//! `--defs NAME[,NAME...]`: which definitions each name binds, with a spelling that pins each one.
use crate::domain::{CorpusView, IngestResult, Interner, NodeId};
use crate::graph::Graph;
use crate::navigate::{SymbolLookup, pinning_selector};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{self, Write};

pub const MAX_DEFS_NAMES: usize = 16;
pub const MAX_DEFS_PER_NAME: usize = 25;

pub struct DefsRequest<'a> {
    pub root: &'a str,
    pub names: &'a [&'a str],
}

/// Split a comma-separated `--defs` argument into trimmed, non-empty names.
#[must_use]
pub fn split_defs_names(names: &str) -> Vec<&str> {
    names
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .collect()
}

pub fn execute_defs<W: Write>(
    request: &DefsRequest<'_>,
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    out: &mut W,
) -> io::Result<()> {
    if request.names.is_empty() || request.names.len() > MAX_DEFS_NAMES {
        return Err(io::Error::other(format!(
            "ripwire-rs: --defs takes 1..{MAX_DEFS_NAMES} names, got {}",
            request.names.len()
        )));
    }
    let val = call_defs(request, ingest, graph, interner);
    crate::serialize::serialize_defs_xml(&val, out)
}

#[must_use]
pub fn call_defs(
    request: &DefsRequest<'_>,
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
) -> Value {
    let view = (ingest, interner);
    let mut matched: Vec<Vec<NodeId>> = Vec::with_capacity(request.names.len());
    let mut missing: Vec<&str> = Vec::new();
    for &name in request.names {
        let mut ids: Vec<NodeId> = view
            .find_symbols(name)
            .into_iter()
            .filter(|&id| ingest.sym(id).is_some_and(|s| s.is_code_symbol()))
            .collect();
        ids.sort_by_key(|&id| ingest.sym(id).map(|s| (view.sym_path(s), s.line, id)));
        if ids.is_empty() {
            missing.push(name);
        }
        matched.push(ids);
    }
    let same_name = same_name_index(ingest, &matched);
    let total: usize = matched.iter().map(Vec::len).sum();
    let mut defs: Vec<Value> = Vec::new();
    for ids in &matched {
        for &id in ids.iter().take(MAX_DEFS_PER_NAME) {
            if let Some(row) = def_row(id, &same_name, ingest, graph, interner) {
                defs.push(row);
            }
        }
    }
    json!({
        "root": request.root,
        "names": request.names.len(),
        "shown": defs.len(),
        "total": total,
        "defs": defs,
        "missing": missing,
    })
}

/// Every symbol sharing a name with a matched definition, so a pinning spelling is checked against all rivals.
fn same_name_index(
    ingest: &IngestResult,
    matched: &[Vec<NodeId>],
) -> HashMap<lasso::Spur, Vec<NodeId>> {
    let mut index: HashMap<lasso::Spur, Vec<NodeId>> = matched
        .iter()
        .flatten()
        .filter_map(|&id| ingest.sym(id))
        .map(|s| (s.name, Vec::new()))
        .collect();
    for sym in &ingest.symbols {
        if let Some(rivals) = index.get_mut(&sym.name) {
            rivals.push(sym.id);
        }
    }
    index
}

fn def_row(
    id: NodeId,
    same_name: &HashMap<lasso::Spur, Vec<NodeId>>,
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
) -> Option<Value> {
    let view = (ingest, interner);
    let sym = ingest.sym(id)?;
    let rivals = same_name.get(&sym.name).map_or(&[][..], Vec::as_slice);
    let sel = (rivals.len() > 1)
        .then(|| pinning_selector(sym, rivals, ingest, interner))
        .flatten();
    Some(json!({
        "name": view.sym_name(sym),
        "scope": view.sym_scope(sym),
        "kind": sym.kind.tag(),
        "path": view.sym_path(sym),
        "line": sym.line,
        "lang": sym.lang.tag(),
        "test": view.is_test_symbol(sym),
        "callers": graph.in_degree(id),
        "sel": sel,
    }))
}
