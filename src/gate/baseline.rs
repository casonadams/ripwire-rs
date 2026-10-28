use crate::domain::{IngestResult, Interner, SymKind};
use crate::gate::rename::BodyFingerprint;
use crate::graph::Graph;
use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{self, Write};
use std::path::Path;

pub const DEFAULT_BASELINE_FILE: &str = ".ripwire-rs_quality_baseline";

/// Snapshot of repository code quality metrics at a specific commit.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct QualitySnapshot {
    pub head_sha: String,
    pub dirty: bool,
    pub absorbed: usize,
    pub ccx_by_sym: HashMap<String, u32>,
    pub loc_by_sym: HashMap<String, u32>,
    pub cx_by_sym: HashMap<String, u32>,
    pub nest_by_sym: HashMap<String, u16>,
    pub params_by_sym: HashMap<String, u16>,
    pub dead_symbols: HashSet<String>,
    pub public_api: HashSet<String>,
    pub line_by_sym: HashMap<String, u32>,
    /// Body shapes of the callables in the changed files, for rename pairing. In memory only: never
    /// serialized, so a pinned sidecar baseline carries none and pairs no renames.
    pub body_by_sym: HashMap<String, BodyFingerprint>,
}

pub(crate) fn symbol_canonical_key(sym_name: &str, file_path: &str, scope: Option<&str>) -> String {
    match scope {
        Some(sc) => format!("{file_path}::{sc}::{sym_name}"),
        None => format!("{file_path}::{sym_name}"),
    }
}

/// What can reach a callable without a resolved call edge, gathered once per snapshot.
struct Reachable {
    /// Every name some reference uses: bound, declined or unresolved calls, reads, values, macro
    /// arguments. Matched by name, so this is a floor.
    referenced: HashSet<lasso::Spur>,
    /// Types that implement or extend another, whose methods may be reached by dispatch.
    dispatched_types: HashSet<lasso::Spur>,
}

impl Reachable {
    fn of(ingest: &IngestResult) -> Self {
        let referenced = ingest.references.iter().map(|r| r.callee_name).collect();
        let dispatched_types = ingest
            .references
            .iter()
            .filter(|r| r.role == crate::domain::RefRole::Extends)
            .filter_map(|r| r.receiver_type.or(r.qualifier))
            .collect();
        Self {
            referenced,
            dispatched_types,
        }
    }
}

/// Constructors are reached through their type: `constructor`, `__init__`, `__new__`, or a method
/// named after its type.
fn is_constructor(name: &str, scope: Option<&str>) -> bool {
    matches!(name, "constructor" | "__init__" | "__new__")
        || scope.is_some_and(|sc| sc.rsplit("::").next() == Some(name))
}

/// A function or method with a body that nothing references, outside headers and tests, that is
/// not an entrypoint, a constructor, a method of a type that implements or extends another, or a
/// method with no enclosing type (an object-literal member).
fn is_unreferenced_callable(
    sym: &crate::domain::Symbol,
    path: &str,
    graph: &Graph,
    interner: &Interner,
    reachable: &Reachable,
) -> bool {
    let semantics = sym.lang.semantics();
    let name = &interner[sym.name];
    let scope = sym.scope.map(|sc| &interner[sc]);
    let method_reached = sym.kind == SymKind::Method
        && sym
            .scope
            .is_none_or(|sc| reachable.dispatched_types.contains(&sc));
    matches!(sym.kind, SymKind::Function | SymKind::Method)
        && sym.sig_end_byte < sym.end_byte
        && graph.in_degree(sym.id) == 0
        && !reachable.referenced.contains(&sym.name)
        && !method_reached
        && !is_constructor(name, scope)
        && !semantics.is_entrypoint_symbol(name, scope, sym.kind, path)
        && !semantics.is_header_file(path)
        && !crate::gate::affected::is_test_path(path)
}

/// Compute a quality snapshot from an ingested repository index.
#[must_use]
pub fn compute_snapshot(
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    head_sha: &str,
) -> QualitySnapshot {
    let mut snap = QualitySnapshot {
        head_sha: head_sha.to_string(),
        dirty: false,
        absorbed: 0,
        ..Default::default()
    };

    let reachable = Reachable::of(ingest);
    for sym in &ingest.symbols {
        if sym.kind == SymKind::Section || sym.kind == SymKind::ModuleScope {
            continue;
        }

        let name = &interner[sym.name];
        let path = ingest.sym_file_path(sym, interner);
        let scope = sym.scope.map(|sc| &interner[sc]);
        let key = symbol_canonical_key(name, path, scope);

        snap.line_by_sym
            .entry(key.clone())
            .and_modify(|v| *v = (*v).min(sym.line))
            .or_insert(sym.line);
        snap.ccx_by_sym
            .entry(key.clone())
            .and_modify(|v| *v = (*v).max(sym.ccx))
            .or_insert(sym.ccx);

        snap.cx_by_sym
            .entry(key.clone())
            .and_modify(|v| *v = (*v).max(sym.cx))
            .or_insert(sym.cx);

        snap.loc_by_sym
            .entry(key.clone())
            .and_modify(|v| *v = (*v).max(sym.loc))
            .or_insert(sym.loc);

        snap.nest_by_sym
            .entry(key.clone())
            .and_modify(|v| *v = (*v).max(sym.max_nest))
            .or_insert(sym.max_nest);

        snap.params_by_sym
            .entry(key.clone())
            .and_modify(|v| *v = (*v).max(sym.params))
            .or_insert(sym.params);

        if is_unreferenced_callable(sym, path, graph, interner, &reachable) {
            snap.dead_symbols.insert(key.clone());
        }

        if sym.lang.semantics().is_header_file(path) {
            snap.public_api.insert(key);
        }
    }

    snap
}

/// Serialize snapshot to standard line-oriented text format.
#[must_use]
pub fn serialize_snapshot_to_string(s: &QualitySnapshot) -> String {
    let mut out = String::new();
    out.push_str("# ripwire-rs quality baseline v1 — do not hand-edit\n");
    if !s.head_sha.is_empty() {
        out.push_str(&format!("head {}\n", s.head_sha));
    }
    if s.dirty {
        out.push_str("dirty 1\n");
        out.push_str(&format!("absorbed {}\n", s.absorbed));
    }

    let mut ccx_keys: Vec<_> = s.ccx_by_sym.keys().collect();
    ccx_keys.sort_unstable();
    for k in ccx_keys {
        out.push_str(&format!("ccx {} {}\n", k, s.ccx_by_sym[k]));
    }

    let mut cx_keys: Vec<_> = s.cx_by_sym.keys().collect();
    cx_keys.sort_unstable();
    for k in cx_keys {
        out.push_str(&format!("cx {} {}\n", k, s.cx_by_sym[k]));
    }

    let mut loc_keys: Vec<_> = s.loc_by_sym.keys().collect();
    loc_keys.sort_unstable();
    for k in loc_keys {
        out.push_str(&format!("loc {} {}\n", k, s.loc_by_sym[k]));
    }

    let mut nest_keys: Vec<_> = s.nest_by_sym.keys().collect();
    nest_keys.sort_unstable();
    for k in nest_keys {
        out.push_str(&format!("nest {} {}\n", k, s.nest_by_sym[k]));
    }

    let mut param_keys: Vec<_> = s.params_by_sym.keys().collect();
    param_keys.sort_unstable();
    for k in param_keys {
        out.push_str(&format!("params {} {}\n", k, s.params_by_sym[k]));
    }

    let mut dead: Vec<_> = s.dead_symbols.iter().collect();
    dead.sort_unstable();
    for k in dead {
        out.push_str(&format!("dead {k}\n"));
    }

    let mut pub_api: Vec<_> = s.public_api.iter().collect();
    pub_api.sort_unstable();
    for k in pub_api {
        out.push_str(&format!("public {k}\n"));
    }

    out
}

fn parse_metric_pair<T: std::str::FromStr>(
    parts: &mut std::str::SplitWhitespace<'_>,
    map: &mut HashMap<String, T>,
) {
    if let (Some(k), Some(v)) = (parts.next(), parts.next().and_then(|v| v.parse().ok())) {
        map.insert(k.to_string(), v);
    }
}

fn parse_line_record(line: &str, snap: &mut QualitySnapshot) {
    let mut parts = line.split_whitespace();
    let Some(tag) = parts.next() else {
        return;
    };
    match tag {
        "head" => snap.head_sha = parts.next().unwrap_or("").to_string(),
        "dirty" => snap.dirty = true,
        "absorbed" => snap.absorbed = parts.next().and_then(|v| v.parse().ok()).unwrap_or(0),
        "ccx" => parse_metric_pair(&mut parts, &mut snap.ccx_by_sym),
        "cx" => parse_metric_pair(&mut parts, &mut snap.cx_by_sym),
        "loc" => parse_metric_pair(&mut parts, &mut snap.loc_by_sym),
        "nest" => parse_metric_pair(&mut parts, &mut snap.nest_by_sym),
        "params" => parse_metric_pair(&mut parts, &mut snap.params_by_sym),
        "dead" => {
            if let Some(k) = parts.next() {
                snap.dead_symbols.insert(k.to_string());
            }
        }
        "public" => {
            if let Some(k) = parts.next() {
                snap.public_api.insert(k.to_string());
            }
        }
        _ => {}
    }
}

/// Parse snapshot from standard line-oriented text format.
#[must_use]
pub fn parse_snapshot_from_str(content: &str) -> QualitySnapshot {
    let mut snap = QualitySnapshot::default();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') || trimmed.is_empty() {
            continue;
        }
        parse_line_record(trimmed, &mut snap);
    }
    snap
}

/// Write quality baseline file atomically to disk.
pub fn write_baseline_file(snap: &QualitySnapshot, path: &Path) -> io::Result<()> {
    let content = serialize_snapshot_to_string(snap);
    let tmp_path = path.with_extension("tmp");
    let mut f = fs::File::create(&tmp_path)?;
    f.write_all(content.as_bytes())?;
    f.sync_all()?;
    fs::rename(tmp_path, path)
}

/// Read quality baseline file from disk.
pub fn read_baseline_file(path: &Path) -> io::Result<QualitySnapshot> {
    let content = fs::read_to_string(path)?;
    Ok(parse_snapshot_from_str(&content))
}
