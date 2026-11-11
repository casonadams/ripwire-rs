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

/// Which definitions a bare name written in one file can mean: a file that defines or binds the name
/// itself means its own, not a definition in another file.
pub(crate) struct NameScope {
    defined: Vec<HashSet<lasso::Spur>>,
    bound: Vec<HashSet<lasso::Spur>>,
    scopes: crate::resolve::LocalScopes,
}

impl NameScope {
    pub(crate) fn of(ingest: &IngestResult) -> Self {
        let mut defined = vec![HashSet::new(); ingest.files.len()];
        for sym in &ingest.symbols {
            if let Some(names) = defined.get_mut(sym.file_id as usize) {
                names.insert(sym.name);
            }
        }
        let mut bound = vec![HashSet::new(); ingest.files.len()];
        let mut scopes = crate::resolve::LocalScopes::new();
        for binding in &ingest.local_bindings {
            if let Some(names) = bound.get_mut(binding.file_id as usize) {
                names.insert(binding.name);
            }
            scopes
                .entry((binding.file_id, binding.name))
                .or_default()
                .push((binding.scope_start, binding.scope_end));
        }
        Self {
            defined,
            bound,
            scopes,
        }
    }

    /// True when a bare `name` written at `byte` in `from` can mean the definition in `target`: the
    /// file is `target`, or neither defines the name nor binds it in a scope around `byte`.
    pub(crate) fn reaches_at(&self, from: u32, target: u32, name: lasso::Spur, byte: u32) -> bool {
        from == target
            || !(self
                .defined
                .get(from as usize)
                .is_some_and(|names| names.contains(&name))
                || self.scopes.get(&(from, name)).is_some_and(|spans| {
                    spans
                        .iter()
                        .any(|&(start, end)| start <= byte && byte < end)
                }))
    }

    /// As [`Self::reaches_at`] for a use whose position is unknown: any binding in the file shadows.
    pub(crate) fn reaches(&self, from: u32, target: u32, name: lasso::Spur) -> bool {
        from == target
            || !(self
                .defined
                .get(from as usize)
                .is_some_and(|names| names.contains(&name))
                || self
                    .bound
                    .get(from as usize)
                    .is_some_and(|names| names.contains(&name)))
    }
}

/// What can reach a callable without a resolved call edge, gathered once per snapshot.
struct Reachable {
    /// Names some reference uses that no local definition can shadow: qualified, member and
    /// macro-argument references, and every reference in a language that does not shadow by name.
    /// Matched by name, so this is a floor.
    referenced: HashSet<lasso::Spur>,
    /// Per bare name in a language where a local definition shadows it, the files that mention it.
    bare_in: BareMentions,
    scope: NameScope,
    /// Supertypes per type: the traits it implements and the classes it extends.
    supertypes: HashMap<lasso::Spur, Vec<lasso::Spur>>,
    /// Member names per indexed type, to tell which methods a supertype declares.
    members: HashMap<lasso::Spur, HashSet<lasso::Spur>>,
    /// Trait implementation blocks per file with their trait: `impl Trait for T { .. }`.
    trait_impls: HashMap<u32, Vec<(u32, u32, lasso::Spur)>>,
    /// Types with an implementation record in a file: their trait methods are told by span.
    implemented_in: HashSet<(u32, lasso::Spur)>,
}

impl Reachable {
    fn of(ingest: &IngestResult) -> Self {
        let mut supertypes = extends_supertypes(ingest);
        let (trait_impls, implemented_in) = implementation_records(ingest, &mut supertypes);
        let (referenced, bare_in) = split_references(ingest);
        Self {
            referenced,
            bare_in,
            scope: NameScope::of(ingest),
            supertypes,
            members: members_by_scope(ingest),
            trait_impls,
            implemented_in,
        }
    }

    /// True when some reference can mean `sym`: a name nothing shadows, or a bare mention from its
    /// own file or from a file that does not define the name itself.
    fn mentions(&self, sym: &crate::domain::Symbol) -> bool {
        self.referenced.contains(&sym.name)
            || self.bare_in.get(&sym.name).is_some_and(|mentions| {
                mentions
                    .iter()
                    .any(|&(file, byte)| self.scope.reaches_at(file, sym.file_id, sym.name, byte))
            })
    }

    /// A supertype reaches `name` when it lies outside the tree (its members are unknown) or
    /// declares a member of that name.
    fn supertype_declares(&self, supertype: lasso::Spur, name: lasso::Spur) -> bool {
        self.members
            .get(&supertype)
            .is_none_or(|declared| declared.contains(&name))
    }

    /// A member of `scope` reached by dispatch through a supertype. Where the type has an
    /// implementation record in the file, only methods inside a trait implementation count, so
    /// inherent methods stay checked.
    fn dispatches_to(&self, sym: &crate::domain::Symbol, scope: lasso::Spur) -> bool {
        if self.implemented_in.contains(&(sym.file_id, scope)) {
            return self.trait_impls.get(&sym.file_id).is_some_and(|impls| {
                impls.iter().any(|&(start, end, trait_name)| {
                    start <= sym.sig_start_byte
                        && sym.end_byte <= end
                        && self.supertype_declares(trait_name, sym.name)
                })
            });
        }
        self.supertypes
            .get(&scope)
            .is_some_and(|supers| supers.iter().any(|&s| self.supertype_declares(s, sym.name)))
    }
}

type TraitImpls = HashMap<u32, Vec<(u32, u32, lasso::Spur)>>;

fn extends_supertypes(ingest: &IngestResult) -> HashMap<lasso::Spur, Vec<lasso::Spur>> {
    let mut supertypes: HashMap<lasso::Spur, Vec<lasso::Spur>> = HashMap::new();
    for r in &ingest.references {
        if r.role == crate::domain::RefRole::Extends
            && let Some(ty) = r.receiver_type.or(r.qualifier)
        {
            supertypes.entry(ty).or_default().push(r.callee_name);
        }
    }
    supertypes
}

/// Per-file trait implementation spans and the types with an implementation record; trait
/// implementations also add to `supertypes`.
fn implementation_records(
    ingest: &IngestResult,
    supertypes: &mut HashMap<lasso::Spur, Vec<lasso::Spur>>,
) -> (TraitImpls, HashSet<(u32, lasso::Spur)>) {
    let mut trait_impls: TraitImpls = HashMap::new();
    let mut implemented_in = HashSet::new();
    for imp in &ingest.implementations {
        implemented_in.insert((imp.file_id, imp.subject));
        if let Some(trait_name) = imp.trait_name {
            supertypes.entry(imp.subject).or_default().push(trait_name);
            trait_impls.entry(imp.file_id).or_default().push((
                imp.start_byte,
                imp.end_byte,
                trait_name,
            ));
        }
    }
    (trait_impls, implemented_in)
}

fn members_by_scope(ingest: &IngestResult) -> HashMap<lasso::Spur, HashSet<lasso::Spur>> {
    let mut members: HashMap<lasso::Spur, HashSet<lasso::Spur>> = HashMap::new();
    for s in &ingest.symbols {
        if let Some(scope) = s.scope {
            members.entry(scope).or_default().insert(s.name);
        }
    }
    members
}

/// Per name, the `(file, byte)` positions of its bare mentions.
type BareMentions = HashMap<lasso::Spur, Vec<(u32, u32)>>;

/// Splits reference names into those no local definition can shadow and, per shadowable bare name,
/// the `(file, byte)` positions that mention it.
fn split_references(ingest: &IngestResult) -> (HashSet<lasso::Spur>, BareMentions) {
    let mut referenced = HashSet::new();
    let mut bare_in = BareMentions::new();
    for r in &ingest.references {
        let scoped = r.lang.semantics().local_definition_shadows_name()
            && r.qualifier.is_none()
            && !r.member_call
            && !r.macro_arg;
        if scoped {
            bare_in
                .entry(r.callee_name)
                .or_default()
                .push((r.file_id, r.start_byte));
        } else {
            referenced.insert(r.callee_name);
        }
    }
    (referenced, bare_in)
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
    let is_dunder = name.len() > 4 && name.starts_with("__") && name.ends_with("__");
    let method_reached = is_dunder
        || match sym.scope {
            None => sym.kind == SymKind::Method,
            Some(sc) => reachable.dispatches_to(sym, sc),
        };
    matches!(sym.kind, SymKind::Function | SymKind::Method)
        && sym.sig_end_byte < sym.end_byte
        && graph.in_degree(sym.id) == 0
        && !reachable.mentions(sym)
        && !method_reached
        && !sym.registered
        && !is_constructor(name, scope)
        && !semantics.is_entrypoint_symbol(name, scope, sym.kind, path)
        && !semantics.is_header_file(path)
        && !crate::domain::test_path::is_test_path(path)
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
