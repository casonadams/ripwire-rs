pub mod impact;
pub mod inspect;
pub mod query;
pub mod verify;
pub mod xml;

pub use impact::*;
pub use inspect::*;
pub use query::*;
pub use verify::*;
pub use xml::*;

use crate::domain::{IngestResult, Interner, NodeId};
use crate::graph::Graph;
use crate::resolve::EdgeProv;
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};

/// Detailed navigation information for a symbol.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct SymbolNavInfo {
    pub id: NodeId,
    pub name: String,
    pub path: String,
    pub line: u32,
    pub kind: &'static str,
    pub rank: f64,
    pub weight: f32,
    pub tested: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sites: Vec<String>,
    /// Matched query definitions connected to this neighbor, with how each edge bound.
    #[serde(skip)]
    pub of_defs: Vec<(NodeId, EdgeProv)>,
    /// How this callee edge was bound.
    #[serde(skip)]
    pub prov: EdgeProv,
}

/// 1-hop neighborhood of a symbol containing both incoming callers and outgoing callees.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NavNeighborhood {
    pub target: SymbolNavInfo,
    pub callers: Vec<SymbolNavInfo>,
    pub callees: Vec<SymbolNavInfo>,
    #[serde(skip)]
    pub gauge_attrs: String,
}

/// Transitive reachability impact node with hop distance.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ImpactNode {
    pub id: NodeId,
    pub name: String,
    pub path: String,
    pub line: u32,
    pub kind: &'static str,
    pub rank: f64,
    pub hops: u32,
    pub tested: bool,
    /// The edge that first reached this node binds by name alone: a hedge, not a proven caller.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub by_name: bool,
}

fn matches_symbol_selector(
    sym: &crate::domain::Symbol,
    ingest: &IngestResult,
    interner: &Interner,
    selector: &str,
) -> bool {
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let sym_name = view.sym_name(sym);
    let sym_scope = view.sym_scope(sym).unwrap_or("");
    let file_path = view.sym_path(sym);

    if let Some((prefix, name)) = selector.rsplit_once("::") {
        if let Some((file, sc)) = prefix.rsplit_once("::") {
            return sym_name == name && sym_scope == sc && file_path.ends_with(trim_dot(file));
        }
        return sym_name == name && (sym_scope == prefix || file_path.ends_with(trim_dot(prefix)));
    }

    if let Some((file, name)) = selector.split_once(':') {
        return sym_name == name && file_path.ends_with(trim_dot(file));
    }

    if let Some((scope, name)) = selector.split_once('.') {
        return sym_name == name && sym_scope == scope;
    }

    sym_name == selector
}

/// A selector spelling `find_symbols` binds to `sym` alone, shortest first; `None` when no spelling is
/// unique. `same_name` must hold every symbol sharing `sym`'s name.
#[must_use]
pub fn pinning_selector(
    sym: &crate::domain::Symbol,
    same_name: &[NodeId],
    ingest: &IngestResult,
    interner: &Interner,
) -> Option<String> {
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let name = view.sym_name(sym);
    let path = view.sym_path(sym);
    let scope = view.sym_scope(sym);
    let unique = |selector: &str| {
        same_name
            .iter()
            .filter_map(|&id| ingest.sym(id))
            .filter(|s| matches_symbol_selector(s, ingest, interner, selector))
            .map(|s| s.id)
            .eq([sym.id])
    };
    let mut spellings = Vec::with_capacity(4);
    if let Some(sc) = scope {
        spellings.push(format!("{sc}::{name}"));
        spellings.push(format!("{sc}.{name}"));
    }
    spellings.push(format!("{path}:{name}"));
    if let Some(sc) = scope {
        spellings.push(format!("{path}::{sc}::{name}"));
    }
    if let Some(found) = spellings.into_iter().find(|s| unique(s)) {
        return Some(found);
    }
    (view.symbol_at_line(path, sym.line) == Some(sym.id)).then(|| format!("{path}:{}", sym.line))
}

fn trim_dot(path: &str) -> &str {
    path.strip_prefix("./").unwrap_or(path)
}

fn file_line_selector(selector: &str) -> Option<(&str, u32)> {
    let (file, line) = selector.rsplit_once(':')?;
    let line = line.parse().ok()?;
    (!file.is_empty() && !file.ends_with(':')).then(|| (trim_dot(file), line))
}

pub trait SymbolLookup<'a>: crate::domain::CorpusView<'a> {
    #[must_use]
    fn find_symbols(&self, selector: &str) -> Vec<NodeId> {
        let ingest = self.ingest();
        let interner = self.interner();
        if is_seed(selector) {
            return resolve_seed(selector, ingest, interner)
                .map(|seed| vec![seed.id])
                .unwrap_or_default();
        }
        if let Some(target_spur) = interner.get(selector) {
            let exact: Vec<NodeId> = ingest
                .symbols
                .iter()
                .filter(|s| s.name == target_spur)
                .map(|s| s.id)
                .collect();
            if !exact.is_empty() {
                return exact;
            }
        }
        if let Some((file, line)) = file_line_selector(selector) {
            return self.symbol_at_line(file, line).into_iter().collect();
        }

        ingest
            .symbols
            .iter()
            .filter(|s| matches_symbol_selector(s, ingest, interner, selector))
            .map(|s| s.id)
            .collect()
    }

    #[must_use]
    fn symbol_at_line(&self, file: &str, line: u32) -> Option<NodeId> {
        self.ingest()
            .symbols
            .iter()
            .filter(|s| s.line <= line && self.sym_path(s).ends_with(file))
            .max_by_key(|s| (s.line, std::cmp::Reverse(s.id)))
            .map(|s| s.id)
    }

    #[must_use]
    fn find_first_symbol(&self, selector: &str) -> Option<&'a crate::domain::Symbol> {
        let id = *self.find_symbols(selector).first()?;
        self.ingest().sym(id)
    }
}

impl<'a, T: crate::domain::CorpusView<'a> + ?Sized> SymbolLookup<'a> for T {}

/// Find symbol IDs by exact name match or qualified selector (`p::sc::n`, `sc::n`, `file:name`, `Type.method`).
#[must_use]
pub fn find_symbols_by_name(name: &str, ingest: &IngestResult, interner: &Interner) -> Vec<NodeId> {
    (ingest, interner).find_symbols(name)
}

/// Compute bounded Levenshtein edit distance between two ASCII strings.
#[must_use]
pub fn edit_distance_bounded(a: &str, b: &str, max_dist: usize) -> Option<usize> {
    let a_bytes = a.as_bytes();
    let b_bytes = b.as_bytes();
    if a_bytes.len().abs_diff(b_bytes.len()) > max_dist {
        return None;
    }
    let mut prev: Vec<usize> = (0..=b_bytes.len()).collect();
    let mut curr = vec![0; b_bytes.len() + 1];
    for (i, &ca) in a_bytes.iter().enumerate() {
        curr[0] = i + 1;
        let mut row_min = curr[0];
        for (j, &cb) in b_bytes.iter().enumerate() {
            let cost = usize::from(!ca.eq_ignore_ascii_case(&cb));
            curr[j + 1] = (prev[j + 1] + 1).min(curr[j] + 1).min(prev[j] + cost);
            row_min = row_min.min(curr[j + 1]);
        }
        if row_min > max_dist {
            return None;
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    let d = prev[b_bytes.len()];
    (d <= max_dist).then_some(d)
}

/// Find the closest symbol name in the index within edit distance <= 3.
#[must_use]
pub fn find_nearest_symbol_name<'a>(
    query: &str,
    ingest: &'a IngestResult,
    interner: &'a Interner,
) -> Option<&'a str> {
    let target = query
        .rsplit("::")
        .next()
        .unwrap_or(query)
        .rsplit(':')
        .next()
        .unwrap_or(query)
        .rsplit('.')
        .next()
        .unwrap_or(query);
    if target.len() < 3 {
        return None;
    }
    let max_dist = (target.len() / 3).clamp(1, 3);
    let mut best: Option<(usize, &'a str)> = None;
    for sym in &ingest.symbols {
        if sym.kind == crate::domain::SymKind::Section {
            continue;
        }
        let cand = &interner[sym.name];
        if let Some(d) = edit_distance_bounded(target, cand, max_dist)
            && d > 0
            && best.is_none_or(|(bd, bn)| d < bd || (d == bd && cand < bn))
        {
            best = Some((d, cand));
        }
    }
    best.map(|(_, name)| name)
}

/// Build `SymbolNavInfo` for a node.
#[must_use]
pub fn get_symbol_nav_info(
    id: NodeId,
    ingest: &IngestResult,
    ranks: &[f64],
    interner: &Interner,
    weight: f32,
) -> Option<SymbolNavInfo> {
    let sym = ingest.sym(id)?;
    let name = interner[sym.name].to_string();
    let file = ingest.file(sym.file_id)?;
    let path = interner[file.path].to_string();
    let rank = ranks.get(id as usize).copied().unwrap_or(0.0);

    let tested = crate::gate::affected::is_test_path(&path);
    Some(SymbolNavInfo {
        id,
        name,
        path,
        line: sym.line,
        kind: sym.kind.tag(),
        rank,
        weight,
        tested,
        sites: Vec::new(),
        of_defs: Vec::new(),
        prov: EdgeProv::Local,
    })
}

pub fn compute_tested_node_mask(
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
) -> Vec<bool> {
    use crate::domain::CorpusView;
    (ingest, interner).compute_test_reachability(graph)
}

/// Compute 1-hop neighborhood (direct callers and callees) for a target symbol.
#[must_use]
pub fn navigate_1hop(
    sym_id: NodeId,
    ingest: &IngestResult,
    graph: &Graph,
    ranks: &[f64],
    interner: &Interner,
) -> Option<NavNeighborhood> {
    navigate_1hop_multi(&[sym_id], ingest, graph, ranks, interner)
}

/// Compute 1-hop neighborhood unioned across multiple symbol definitions/overloads.
#[must_use]
pub fn navigate_1hop_multi(
    sym_ids: &[NodeId],
    ingest: &IngestResult,
    graph: &Graph,
    ranks: &[f64],
    interner: &Interner,
) -> Option<NavNeighborhood> {
    let tested_mask = compute_tested_node_mask(ingest, graph, interner);
    let &first_id = sym_ids.first()?;
    let mut target = get_symbol_nav_info(first_id, ingest, ranks, interner, 1.0)?;
    target.tested |= tested_mask.get(first_id as usize).copied().unwrap_or(false);
    let mut caller_slots: HashMap<NodeId, usize> = HashMap::new();
    let mut callers: Vec<SymbolNavInfo> = Vec::new();
    let mut callee_slots: HashMap<NodeId, usize> = HashMap::new();
    let mut callees: Vec<SymbolNavInfo> = Vec::new();
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let target_names: Vec<_> = sym_ids
        .iter()
        .filter_map(|&id| ingest.sym(id).map(|s| s.name))
        .collect();
    for &sym_id in sym_ids {
        for (caller_id, weight) in graph.callers(sym_id) {
            if let Some(&slot) = caller_slots.get(&caller_id) {
                callers[slot].of_defs.push((
                    sym_id,
                    graph.edge_prov(caller_id, sym_id).unwrap_or_default(),
                ));
            } else if let Some(mut info) =
                get_symbol_nav_info(caller_id, ingest, ranks, interner, weight)
            {
                info.tested |= tested_mask
                    .get(caller_id as usize)
                    .copied()
                    .unwrap_or(false);
                info.sites = view.caller_site_locations(caller_id, &target_names);
                info.of_defs.push((
                    sym_id,
                    graph.edge_prov(caller_id, sym_id).unwrap_or_default(),
                ));
                caller_slots.insert(caller_id, callers.len());
                callers.push(info);
            }
        }
        for edge in graph.callee_edges(sym_id) {
            let callee_id = edge.to;
            if let Some(&slot) = callee_slots.get(&callee_id) {
                callees[slot].of_defs.push((sym_id, edge.prov));
                callees[slot].prov = callees[slot].prov.max(edge.prov);
            } else if let Some(mut info) =
                get_symbol_nav_info(callee_id, ingest, ranks, interner, edge.weight)
            {
                info.prov = edge.prov;
                info.of_defs.push((sym_id, edge.prov));
                info.tested |= tested_mask
                    .get(callee_id as usize)
                    .copied()
                    .unwrap_or(false);
                callee_slots.insert(callee_id, callees.len());
                callees.push(info);
            }
        }
    }
    callers.sort_by(|a, b| b.rank.total_cmp(&a.rank).then_with(|| a.id.cmp(&b.id)));
    callees.sort_by(|a, b| b.rank.total_cmp(&a.rank).then_with(|| a.id.cmp(&b.id)));
    Some(NavNeighborhood {
        target,
        callers,
        callees,
        gauge_attrs: graph.xml_gauge_attrs(),
    })
}

fn create_impact_node(
    curr: NodeId,
    depth: u32,
    ingest: &IngestResult,
    ranks: &[f64],
    interner: &Interner,
) -> Option<ImpactNode> {
    let info = get_symbol_nav_info(curr, ingest, ranks, interner, 1.0)?;
    Some(ImpactNode {
        id: curr,
        name: info.name,
        path: info.path,
        line: info.line,
        kind: info.kind,
        rank: info.rank,
        hops: depth,
        tested: false,
        by_name: false,
    })
}

/// Compute transitive blast radius / caller closure backward along in-edges using BFS.
#[must_use]
pub fn transitive_impact(
    seed_id: NodeId,
    ingest: &IngestResult,
    graph: &Graph,
    ranks: &[f64],
    interner: &Interner,
    max_depth: Option<u32>,
) -> Vec<ImpactNode> {
    transitive_impact_multi(&[seed_id], ingest, graph, ranks, interner, max_depth)
}

fn select_impact_seeds(
    seed_ids: &[NodeId],
    ingest: &IngestResult,
    interner: &Interner,
) -> Vec<NodeId> {
    if seed_ids.len() <= 1 {
        return seed_ids.to_vec();
    }
    let non_test: Vec<NodeId> = seed_ids
        .iter()
        .copied()
        .filter(|&id| {
            ingest.sym(id).is_none_or(|s| {
                !crate::gate::affected::is_test_path(ingest.sym_file_path(s, interner))
            })
        })
        .collect();
    if non_test.is_empty() {
        seed_ids.to_vec()
    } else {
        non_test
    }
}

/// The callers of `curr` an impact walk follows, each with whether its edge binds by name alone.
fn impact_callers(
    curr: NodeId,
    depth: u32,
    graph: &Graph,
) -> impl Iterator<Item = (NodeId, bool)> + '_ {
    let min_weight = if depth == 0 { 0.1 } else { 0.2 };
    let too_busy = depth >= 1 && graph.in_degree(curr) > 512;
    graph
        .callers(curr)
        .filter(move |&(_, weight)| !too_busy && weight >= min_weight)
        .map(move |(caller, _)| {
            let by_name = graph
                .edge_prov(caller, curr)
                .is_some_and(crate::resolve::EdgeProv::is_by_name);
            (caller, by_name)
        })
}

/// Breadth-first caller walk from `seeds` within `max_depth`, as `(node, hops)` in visit order. With
/// `proven_only`, name-only edges are not followed: what it reaches is the proven reach.
fn walk_callers(
    seeds: &[NodeId],
    graph: &Graph,
    max_depth: Option<u32>,
    proven_only: bool,
) -> Vec<(NodeId, u32)> {
    let mut visited = HashSet::new();
    let mut order = Vec::new();
    let mut queue: VecDeque<(NodeId, u32)> = seeds
        .iter()
        .filter(|&&seed| visited.insert(seed))
        .map(|&seed| (seed, 0))
        .collect();
    while let Some((curr, depth)) = queue.pop_front() {
        order.push((curr, depth));
        if max_depth.is_some_and(|limit| depth >= limit) {
            continue;
        }
        for (caller, by_name) in impact_callers(curr, depth, graph) {
            if !(proven_only && by_name) && visited.insert(caller) {
                queue.push_back((caller, depth + 1));
            }
        }
    }
    order
}

#[must_use]
pub fn transitive_impact_multi(
    seed_ids: &[NodeId],
    ingest: &IngestResult,
    graph: &Graph,
    ranks: &[f64],
    interner: &Interner,
    max_depth: Option<u32>,
) -> Vec<ImpactNode> {
    let effective_seeds = select_impact_seeds(seed_ids, ingest, interner);
    let seeds: HashSet<NodeId> = effective_seeds.iter().copied().collect();
    let proven: HashSet<NodeId> = walk_callers(&effective_seeds, graph, max_depth, true)
        .into_iter()
        .map(|(id, _)| id)
        .collect();
    let tested_mask = compute_tested_node_mask(ingest, graph, interner);
    let mut results: Vec<ImpactNode> = walk_callers(&effective_seeds, graph, max_depth, false)
        .into_iter()
        .filter(|(id, _)| !seeds.contains(id))
        .filter_map(|(id, depth)| {
            let mut node = create_impact_node(id, depth, ingest, ranks, interner)?;
            node.by_name = !proven.contains(&id);
            node.tested = tested_mask.get(id as usize).copied().unwrap_or(false)
                || crate::gate::affected::is_test_path(&node.path);
            Some(node)
        })
        .collect();

    results.sort_by(|a, b| {
        a.hops
            .cmp(&b.hops)
            .then_with(|| b.rank.total_cmp(&a.rank))
            .then_with(|| a.path.cmp(&b.path))
            .then_with(|| a.line.cmp(&b.line))
    });
    results
}

/// Split an impact reach into `(tested, untested)` counts. A node is tested when the test mask
/// marked it or its file is a test path.
#[must_use]
pub fn radius_split<'a>(nodes: impl IntoIterator<Item = &'a ImpactNode>) -> (usize, usize) {
    let (mut tested, mut total) = (0, 0);
    for n in nodes {
        total += 1;
        tested += usize::from(n.tested || crate::gate::affected::is_test_path(&n.path));
    }
    (tested, total - tested)
}
