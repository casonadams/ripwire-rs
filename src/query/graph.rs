use crate::domain::{IngestResult, Interner, NodeId};
use crate::graph::Graph;
use crate::query::safety::SafetyContext;
use serde_json::{Value, json};
use std::collections::{HashSet, VecDeque};
use std::io::{self, Write};

pub const DEFAULT_CONNECT_RADIUS: u32 = 2;

pub struct ConnectRequest<'a> {
    pub root: &'a str,
    pub symbols: &'a [&'a str],
    pub radius: u32,
}

pub fn execute_connect_query_with_radius<W: Write>(
    root: &str,
    syms: &str,
    max_radius: u32,
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    out: &mut W,
) -> io::Result<()> {
    let symbols: Vec<&str> = syms
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let request = ConnectRequest {
        root,
        symbols: &symbols,
        radius: max_radius,
    };
    execute_connect(&request, ingest, graph, interner, out)
}

pub fn execute_connect<W: Write>(
    request: &ConnectRequest<'_>,
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    out: &mut W,
) -> io::Result<()> {
    for s in request.symbols {
        if crate::navigate::find_symbols_by_name(s, ingest, interner).is_empty() {
            return Err(io::Error::other(format!(
                "ripwire-rs: --connect symbol not found: {s}"
            )));
        }
    }
    let mut val =
        call_connect_with_radius(ingest, graph, interner, request.symbols, request.radius);
    val["root"] = serde_json::Value::String(request.root.to_string());
    crate::serialize::serialize_connect_xml(&val, out)
}

pub fn split_endpoint_pair(pair: &str) -> (&str, &str) {
    if let Some((from, to)) = pair.split_once(',') {
        (from.trim(), to.trim())
    } else if let Some((from, to)) = pair.split_once("->") {
        (from.trim(), to.trim())
    } else if let Some((from, to)) = pair.rsplit_once(':')
        && !from.ends_with(':')
    {
        (from.trim(), to.trim())
    } else if let Some((from, to)) = pair.split_once(':') {
        (from.trim(), to.trim())
    } else {
        (pair.trim(), "")
    }
}

pub struct PathRequest<'a> {
    pub from: &'a str,
    pub to: &'a str,
}

pub fn execute_path_query<W: Write>(
    pair: &str,
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    out: &mut W,
) -> io::Result<()> {
    let (from, to) = split_endpoint_pair(pair);
    execute_path_between(&PathRequest { from, to }, ingest, graph, interner, out)
}

pub fn execute_path_between<W: Write>(
    request: &PathRequest<'_>,
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    out: &mut W,
) -> io::Result<()> {
    let PathRequest { from, to } = *request;
    if crate::navigate::find_symbols_by_name(from, ingest, interner).is_empty() {
        return Err(io::Error::other(format!(
            "ripwire-rs: --path endpoint not found: {from}"
        )));
    }
    if crate::navigate::find_symbols_by_name(to, ingest, interner).is_empty() {
        return Err(io::Error::other(format!(
            "ripwire-rs: --path endpoint not found: {to}"
        )));
    }
    let val = call_path_between(ingest, graph, interner, from, to);
    crate::serialize::serialize_path_between_xml(&val, out)
}
fn find_symbols_matching(ingest: &IngestResult, interner: &Interner, name: &str) -> Vec<NodeId> {
    use crate::navigate::SymbolLookup;
    (ingest, interner).find_symbols(name)
}

fn bfs_directed_path(graph: &Graph, from: NodeId, to: NodeId) -> Option<Vec<NodeId>> {
    let mut queue = VecDeque::new();
    let mut visited = HashSet::new();
    let mut parent = std::collections::HashMap::new();

    queue.push_back(from);
    visited.insert(from);

    let mut found = false;
    while let Some(curr) = queue.pop_front() {
        if curr == to {
            found = true;
            break;
        }
        for (callee, _) in graph.callees(curr) {
            if visited.insert(callee) {
                parent.insert(callee, curr);
                queue.push_back(callee);
            }
        }
    }

    if !found {
        return None;
    }

    let mut path = Vec::new();
    let mut curr = to;
    while curr != from {
        path.push(curr);
        curr = *parent.get(&curr)?;
    }
    path.push(from);
    path.reverse();
    Some(path)
}

fn find_shortest_path_between_sets(
    graph: &Graph,
    from_ids: &[NodeId],
    to_ids: &[NodeId],
) -> Option<Vec<NodeId>> {
    let mut best: Option<Vec<NodeId>> = None;
    for &f in from_ids {
        for &t in to_ids {
            if f == t && (from_ids.len() > 1 || to_ids.len() > 1) {
                continue;
            }
            if let Some(p) = bfs_directed_path(graph, f, t)
                && best.as_ref().is_none_or(|b| p.len() < b.len())
            {
                best = Some(p);
            }
        }
    }
    best
}

fn build_path_step_json(id: NodeId, ingest: &IngestResult, interner: &Interner) -> Option<Value> {
    let sym = ingest.sym(id)?;
    let path = ingest.sym_file_path(sym, interner);
    Some(json!({
        "name": &interner[sym.name],
        "kind": sym.kind.tag(),
        "path": path,
        "line": sym.line
    }))
}

/// Execute `path_between` tool: shortest directed call path between two symbols.
pub fn call_path_between(
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    from_name: &str,
    to_name: &str,
) -> Value {
    let from_ids = find_symbols_matching(ingest, interner, from_name);
    let to_ids = find_symbols_matching(ingest, interner, to_name);

    if from_ids.is_empty() || to_ids.is_empty() {
        return json!({
            "from": from_name,
            "to": to_name,
            "from_defs": from_ids.len(),
            "to_defs": to_ids.len(),
            "reachable": false,
            "hops": 0,
            "error": "one or both symbols not found"
        });
    }
    if from_ids.len() == 1 && to_ids.len() == 1 && from_ids[0] == to_ids[0] {
        return json!({
            "from": from_name,
            "to": to_name,
            "from_defs": 1,
            "to_defs": 1,
            "reachable": true,
            "hops": 0,
            "path": [from_name]
        });
    }

    let Some(path_ids) = find_shortest_path_between_sets(graph, &from_ids, &to_ids) else {
        return json!({
            "from": from_name,
            "to": to_name,
            "from_defs": from_ids.len(),
            "to_defs": to_ids.len(),
            "reachable": false,
            "hops": 0,
            "path": []
        });
    };

    let path: Vec<&str> = path_ids
        .iter()
        .filter_map(|&id| ingest.sym(id))
        .map(|sym| &interner[sym.name])
        .collect();
    let steps: Vec<Value> = path_ids
        .into_iter()
        .filter_map(|id| build_path_step_json(id, ingest, interner))
        .collect();

    json!({
        "from": from_name,
        "to": to_name,
        "from_defs": from_ids.len(),
        "to_defs": to_ids.len(),
        "reachable": true,
        "hops": path.len().saturating_sub(1),
        "path": path,
        "steps": steps
    })
}

type UndirectedPath = (Vec<NodeId>, Vec<(NodeId, NodeId)>);

fn reconstruct_undirected_path(
    mut curr: NodeId,
    parent: &std::collections::HashMap<NodeId, (NodeId, NodeId, NodeId)>,
) -> UndirectedPath {
    let mut nodes = vec![curr];
    let mut dir_edges = Vec::new();
    while let Some(&(prev, edge_from, edge_to)) = parent.get(&curr) {
        if prev == curr {
            break;
        }
        dir_edges.push((edge_from, edge_to));
        nodes.push(prev);
        curr = prev;
    }
    nodes.reverse();
    dir_edges.reverse();
    (nodes, dir_edges)
}

fn enqueue_undirected_neighbors(
    curr: NodeId,
    depth: u32,
    graph: &Graph,
    parent: &mut std::collections::HashMap<NodeId, (NodeId, NodeId, NodeId)>,
    queue: &mut std::collections::VecDeque<(NodeId, u32)>,
) {
    for (callee, _) in graph.callees(curr) {
        if let std::collections::hash_map::Entry::Vacant(e) = parent.entry(callee) {
            e.insert((curr, curr, callee));
            queue.push_back((callee, depth + 1));
        }
    }
    for (caller, _) in graph.callers(curr) {
        if let std::collections::hash_map::Entry::Vacant(e) = parent.entry(caller) {
            e.insert((curr, caller, curr));
            queue.push_back((caller, depth + 1));
        }
    }
}

fn find_undirected_path_between_sets(
    graph: &Graph,
    from_ids: &[NodeId],
    to_ids: &[NodeId],
    max_radius: u32,
) -> Option<UndirectedPath> {
    let target_set: std::collections::HashSet<NodeId> = to_ids.iter().copied().collect();
    let mut parent: std::collections::HashMap<NodeId, (NodeId, NodeId, NodeId)> =
        std::collections::HashMap::new();
    let mut queue = std::collections::VecDeque::new();
    for &id in from_ids {
        parent.insert(id, (id, id, id));
        queue.push_back((id, 0u32));
    }
    while let Some((curr, depth)) = queue.pop_front() {
        if depth > 0 && target_set.contains(&curr) {
            return Some(reconstruct_undirected_path(curr, &parent));
        }
        if depth < max_radius {
            enqueue_undirected_neighbors(curr, depth, graph, &mut parent, &mut queue);
        }
    }
    None
}

fn build_connect_node_json(
    id: NodeId,
    defs_opt: Option<usize>,
    connects_opt: Option<usize>,
    ingest: &IngestResult,
    interner: &Interner,
) -> Option<Value> {
    let sym = ingest.symbols.get(id as usize)?;
    let name = interner.resolve(&sym.name);
    let path = ingest
        .files
        .get(sym.file_id as usize)
        .map_or("", |f| interner.resolve(&f.path));
    let mut obj = json!({
        "name": name,
        "kind": sym.kind.tag(),
        "path": path,
        "line": sym.line
    });
    if let Some(d) = defs_opt {
        obj["defs"] = json!(d);
    }
    if let Some(c) = connects_opt {
        obj["connects"] = json!(c);
    }
    Some(obj)
}

/// Execute `connect` tool: minimal connecting subgraph relating 2..16 symbols.
struct ConnectAccum {
    max_radius: u32,
    chosen_terminals: Vec<(NodeId, usize)>,
    connections: Vec<Value>,
    all_path_nodes: std::collections::BTreeSet<NodeId>,
    dir_edge_ids: std::collections::BTreeSet<(NodeId, NodeId)>,
}

fn update_connect_terminal(slot: &mut (NodeId, usize), cand_id: NodeId, ingest: &IngestResult) {
    let same_name = ingest
        .sym(slot.0)
        .zip(ingest.sym(cand_id))
        .is_some_and(|(cur, cand)| cur.name == cand.name);
    if same_name {
        slot.0 = cand_id;
    }
}

fn record_connect_pair(
    i: usize,
    j: usize,
    resolved: &[(&str, Vec<NodeId>)],
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    acc: &mut ConnectAccum,
) {
    let (name_a, ref ids_a) = resolved[i];
    let (name_b, ref ids_b) = resolved[j];
    let Some((nodes, d_edges)) =
        find_undirected_path_between_sets(graph, ids_a, ids_b, acc.max_radius)
    else {
        return;
    };
    if let Some(&first_id) = nodes.first() {
        update_connect_terminal(&mut acc.chosen_terminals[i], first_id, ingest);
    }
    if let Some(&last_id) = nodes.last() {
        update_connect_terminal(&mut acc.chosen_terminals[j], last_id, ingest);
    }
    let path_names: Vec<&str> = nodes
        .iter()
        .filter_map(|&id| ingest.symbols.get(id as usize))
        .map(|s| interner.resolve(&s.name))
        .collect();
    acc.connections
        .push(json!({ "from": name_a, "to": name_b, "path": path_names }));
    acc.all_path_nodes.extend(nodes);
    acc.dir_edge_ids.extend(d_edges);
}

fn expand_connect_seeds(ingest: &IngestResult, ids: Vec<NodeId>) -> Vec<NodeId> {
    let mut out = ids.clone();
    for &id in &ids {
        let Some(sym) = ingest.sym(id) else {
            continue;
        };
        if matches!(
            sym.kind,
            crate::domain::SymKind::Class
                | crate::domain::SymKind::Struct
                | crate::domain::SymKind::Interface
        ) {
            for m in &ingest.symbols {
                if m.scope == Some(sym.name) {
                    out.push(m.id);
                }
            }
        }
    }
    out
}

fn call_connect_with_radius(
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    symbol_names: &[&str],
    max_radius: u32,
) -> Value {
    let resolved: Vec<(&str, Vec<NodeId>)> = symbol_names
        .iter()
        .filter_map(|name| {
            let raw_ids = find_symbols_matching(ingest, interner, name);
            (!raw_ids.is_empty()).then(|| (*name, expand_connect_seeds(ingest, raw_ids)))
        })
        .collect();
    let mut acc = ConnectAccum {
        max_radius,
        chosen_terminals: resolved
            .iter()
            .map(|(_, ids)| (ids[0], ids.len()))
            .collect(),
        connections: Vec::new(),
        all_path_nodes: std::collections::BTreeSet::new(),
        dir_edge_ids: std::collections::BTreeSet::new(),
    };

    for i in 0..resolved.len() {
        for j in (i + 1)..resolved.len() {
            record_connect_pair(i, j, &resolved, ingest, graph, interner, &mut acc);
        }
    }

    let term_id_set: std::collections::HashSet<NodeId> =
        acc.chosen_terminals.iter().map(|&(id, _)| id).collect();
    let terminals_json: Vec<Value> = acc
        .chosen_terminals
        .iter()
        .filter_map(|&(id, defs)| build_connect_node_json(id, Some(defs), None, ingest, interner))
        .collect();
    let steiner_json: Vec<Value> = acc
        .all_path_nodes
        .into_iter()
        .filter(|id| !term_id_set.contains(id))
        .filter_map(|id| {
            let deg = graph.in_degree(id) + graph.out_degree(id);
            build_connect_node_json(id, None, Some(deg), ingest, interner)
        })
        .collect();
    let edges_json: Vec<Value> = acc
        .dir_edge_ids
        .into_iter()
        .filter_map(|(u, v)| {
            let su = ingest.symbols.get(u as usize)?;
            let sv = ingest.symbols.get(v as usize)?;
            Some(json!({
                "from": interner.resolve(&su.name),
                "to": interner.resolve(&sv.name)
            }))
        })
        .collect();

    json!({
        "radius": max_radius,
        "symbols": symbol_names,
        "resolved": resolved.len(),
        "terminals": terminals_json,
        "steiner": steiner_json,
        "edges": edges_json,
        "connections": acc.connections
    })
}

/// Execute `situational_awareness` tool.
fn collect_ranked_cochange_partners(
    facts: &SafetyContext<'_>,
    changed_files: &[String],
    commits: Option<&[crate::git::history::CommitInfo]>,
    page: crate::serialize::PageRequest,
) -> Vec<String> {
    let SafetyContext {
        ingest,
        graph,
        interner,
        ..
    } = *facts;
    let coupling = crate::git::cochange::StaticCouplingMap::build(ingest, interner, Some(graph));
    let opts = crate::git::cochange::CochangeOptions::default();
    let commits_empty = Vec::new();
    let commit_slice = commits.unwrap_or(&commits_empty);

    let mut partner_deg: std::collections::HashMap<String, f64> = std::collections::HashMap::new();
    for file_path in changed_files {
        let partners = crate::git::cochange::mine_cochange_for_file(
            file_path,
            commit_slice,
            Some(&coupling),
            Some(ingest),
            Some(interner),
            &opts,
        );
        for p in partners {
            if !changed_files.iter().any(|c| c == &p.file_path) {
                let entry = partner_deg.entry(p.file_path).or_insert(p.deg);
                *entry = entry.max(p.deg);
            }
        }
    }
    let mut ranked: Vec<(String, f64)> = partner_deg.into_iter().collect();
    ranked.sort_by(|a, b| {
        b.1.partial_cmp(&a.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(&b.0))
    });
    let labels: Vec<String> = ranked
        .into_iter()
        .map(|(path, deg)| format!("{path} ({:.0}%)", (deg * 100.0).round()))
        .collect();
    let window = crate::serialize::Paged::new(&labels, page, |l| {
        crate::serialize::text_bytes(&[l.as_str()])
    })
    .window;
    labels
        .into_iter()
        .skip(window.begin)
        .take(window.shown)
        .collect()
}

fn format_affected_test_label(t: &crate::gate::affected::AffectedTest) -> String {
    let evidence = if t.is_direct_change {
        "[changed]".to_string()
    } else if t.is_partner && t.hops == 0 {
        "[partner]".to_string()
    } else if t.is_partner {
        format!("[partner hops={}]", t.hops)
    } else {
        format!("[hops={}]", t.hops)
    };
    format!("{evidence} {}", t.path)
}

/// Execute `situational_awareness` tool.
pub fn call_situational_awareness(
    facts: &SafetyContext<'_>,
    changed_files: &[String],
    commits: Option<&[crate::git::history::CommitInfo]>,
    page: crate::serialize::PageRequest,
) -> Value {
    let SafetyContext {
        ingest,
        graph,
        interner,
        ..
    } = *facts;
    let specs: Vec<&str> = changed_files.iter().map(|s| s.as_str()).collect();
    let (affected_tests, blast_radius) = if specs.is_empty() {
        (Vec::new(), 0)
    } else {
        let report = crate::gate::affected::find_affected_tests(&specs, ingest, graph, interner);
        let tests = report
            .tests
            .iter()
            .map(format_affected_test_label)
            .collect();
        (tests, report.reached_symbols)
    };
    let cochange_partners = collect_ranked_cochange_partners(facts, changed_files, commits, page);

    json!({
        "files_touched": changed_files,
        "tests_to_run": affected_tests,
        "blast_radius": blast_radius,
        "forgotten_partners": cochange_partners
    })
}

fn parse_trace_file_line_token(word: &str) -> Option<(&str, u32)> {
    let clean = word.trim_matches(|c: char| matches!(c, '(' | ')' | '[' | ']' | ',' | ';' | ':'));
    let (file_part, rest) = clean.split_once(':')?;
    if !file_part.contains('.') && !file_part.contains('/') {
        return None;
    }
    let line_str = rest.split(':').next()?;
    let line_num = line_str.parse::<u32>().ok()?;
    Some((file_part, line_num))
}

fn resolve_trace_line_symbol<'a>(
    trimmed: &str,
    ingest: &'a IngestResult,
    interner: &Interner,
) -> Option<&'a crate::domain::Symbol> {
    for word in trimmed.split_whitespace() {
        if let Some((file_part, line_num)) = parse_trace_file_line_token(word)
            && let Some(sym) =
                crate::navigate::resolve_symbol_at(file_part, line_num, ingest, interner)
        {
            return Some(sym);
        }
    }
    let tokens: HashSet<&str> = trimmed
        .split(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .filter(|t| t.len() >= 3)
        .collect();
    ingest
        .symbols
        .iter()
        .filter(|sym| tokens.contains(interner.resolve(&sym.name)))
        .max_by_key(|sym| interner.resolve(&sym.name).len())
}

/// Execute `from_trace` tool.
pub fn call_from_trace(ingest: &IngestResult, interner: &Interner, trace_text: &str) -> Value {
    let mut suspects = Vec::new();

    for line in trace_text.lines() {
        let trimmed = line.trim();
        if let Some(sym) = resolve_trace_line_symbol(trimmed, ingest, interner) {
            let name = interner.resolve(&sym.name);
            let file_path = ingest.sym_file_path(sym, interner);
            suspects.push(json!({
                "name": name,
                "kind": sym.kind.tag(),
                "file": file_path,
                "line": sym.line,
                "trace_line": trimmed
            }));
        }
    }

    json!({
        "matched_frames": suspects.len(),
        "suspects": suspects
    })
}
