use crate::domain::{IngestResult, Interner, NodeId, SymKind};
use crate::graph::Graph;

fn community_anchor_priority<'a, V: crate::domain::CorpusView<'a> + ?Sized>(
    view: &V,
    s: &crate::domain::Symbol,
) -> (u8, u32) {
    let prod = view.is_production_symbol(s);
    let tier = if prod && (s.is_callable() || s.kind.is_type_container()) {
        3
    } else if prod {
        2
    } else if s.is_code_symbol() {
        1
    } else {
        0
    };
    (tier, s.loc.saturating_add(s.ccx.saturating_mul(2)))
}

pub fn build_community_module<'a>(
    id: u32,
    nodes: &[NodeId],
    ingest: &'a IngestResult,
    interner: &'a Interner,
) -> crate::serialize::CommunityModule<'a> {
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let mut members = Vec::new();
    let mut top_dir = "";
    let mut top_label = "";
    let mut best_prio = (0u8, 0u32);
    for &nid in nodes {
        if let Some(s) = ingest.sym(nid) {
            let n = view.sym_name(s);
            let p = view.sym_path(s);
            let prio = community_anchor_priority(&view, s);
            if top_label.is_empty() || prio > best_prio {
                best_prio = prio;
                top_dir = p;
                top_label = n;
            }
            members.push((s.kind.tag(), n, p, s.line));
        }
    }
    crate::serialize::CommunityModule {
        id,
        size: members.len(),
        dir: top_dir,
        label: top_label,
        members,
    }
}

fn tally_isolated_symbols(
    ingest: &IngestResult,
    graph: &Graph,
    sum: &mut crate::serialize::CommunitySummary<'_>,
) {
    for sym in &ingest.symbols {
        if graph.in_degree(sym.id) > 0 || graph.out_degree(sym.id) > 0 {
            continue;
        }
        sum.isolated += 1;
        if sym.kind == crate::domain::SymKind::Section {
            sum.isolated_doc += 1;
        } else if sym.sig_end_byte >= sym.end_byte {
            sum.isolated_decl += 1;
        } else {
            sum.isolated_source += 1;
        }
    }
}

pub fn collect_community_bridges<'a>(
    comms: &[u32],
    modules: &[crate::serialize::CommunityModule<'a>],
    ingest: &'a IngestResult,
    graph: &Graph,
) -> Vec<(u32, u32, &'a str, &'a str, usize)> {
    let label_by_id: std::collections::HashMap<u32, &'a str> =
        modules.iter().map(|m| (m.id, m.label)).collect();
    let mut bridge_counts: std::collections::HashMap<(u32, u32), usize> =
        std::collections::HashMap::new();
    for u in 0..ingest.symbols.len() {
        let cu = comms.get(u).copied().unwrap_or(0);
        for (v, _) in graph.callees(u as NodeId) {
            let cv = comms.get(v as usize).copied().unwrap_or(0);
            if cu != cv {
                let pair = if cu < cv { (cu, cv) } else { (cv, cu) };
                *bridge_counts.entry(pair).or_default() += 1;
            }
        }
    }
    let mut bridges: Vec<(u32, u32, &'a str, &'a str, usize)> = bridge_counts
        .into_iter()
        .map(|((a, b), w)| {
            let la = label_by_id.get(&a).copied().unwrap_or("");
            let lb = label_by_id.get(&b).copied().unwrap_or("");
            (a, b, la, lb, w)
        })
        .collect();
    bridges.sort_by(|x, y| y.4.cmp(&x.4).then_with(|| (x.0, x.1).cmp(&(y.0, y.1))));
    bridges
}

pub fn compute_community_summary<'a>(
    comms: &[u32],
    by_comm: &std::collections::HashMap<u32, Vec<NodeId>>,
    modules: &[crate::serialize::CommunityModule<'a>],
    ingest: &'a IngestResult,
    graph: &Graph,
) -> crate::serialize::CommunitySummary<'a> {
    let connected_singletons = by_comm
        .values()
        .filter(|n| n.len() == 1 && (graph.in_degree(n[0]) > 0 || graph.out_degree(n[0]) > 0))
        .count();
    let pr_iters = graph
        .page_rank(&crate::rank::PageRankConfig::default())
        .iterations
        .max(1);
    let mut sum = crate::serialize::CommunitySummary {
        symbols: ingest.symbol_count(),
        connected_singletons,
        pr_iters,
        gauges: graph.gauges(),
        ..Default::default()
    };
    tally_isolated_symbols(ingest, graph, &mut sum);
    sum.bridges = collect_community_bridges(comms, modules, ingest, graph);
    sum
}

pub fn build_ranked_communities<'a>(
    target_id: Option<u32>,
    ingest: &'a IngestResult,
    graph: &Graph,
    interner: &'a Interner,
) -> (
    Vec<u32>,
    std::collections::HashMap<u32, Vec<NodeId>>,
    Vec<crate::serialize::CommunityModule<'a>>,
) {
    let comms = graph.communities();
    let mut by_comm: std::collections::HashMap<u32, Vec<NodeId>> = std::collections::HashMap::new();
    for (node, &c) in comms.comm.iter().enumerate() {
        let is_member = target_id.is_none() || target_id == Some(c);
        let is_code = ingest
            .sym(node as NodeId)
            .is_some_and(|s| s.kind != SymKind::ModuleScope);
        if is_member && is_code {
            by_comm.entry(c).or_default().push(node as NodeId);
        }
    }
    let mut comm_ids: Vec<u32> = by_comm
        .iter()
        .filter(|(_, nodes)| target_id.is_some() || nodes.len() >= 2)
        .map(|(&id, _)| id)
        .collect();
    comm_ids.sort_by(|a, b| {
        by_comm[b]
            .len()
            .cmp(&by_comm[a].len())
            .then_with(|| a.cmp(b))
    });
    let modules = comm_ids
        .into_iter()
        .map(|id| build_community_module(id, &by_comm[&id], ingest, interner))
        .collect();
    (comms.comm, by_comm, modules)
}

/// Per file, the files it declares as child modules (`mod x;`): containment, which is no dependency.
fn collect_containment_edges(
    ingest: &IngestResult,
    interner: &Interner,
    file_index: &std::collections::HashMap<&str, u32>,
) -> Vec<std::collections::BTreeSet<usize>> {
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let f_len = ingest.files.len();
    let mut contained = vec![std::collections::BTreeSet::new(); f_len];
    for s in &ingest.symbols {
        let uf = s.file_id as usize;
        if uf < f_len
            && let Some(mod_spec) = s
                .lang
                .semantics()
                .symbol_implied_import(s.kind, view.sym_name(s))
            && let Some(vf) =
                s.lang
                    .semantics()
                    .resolve_import_target(view.sym_path(s), &mod_spec, file_index)
            && (vf as usize) != uf
            && (vf as usize) < f_len
        {
            contained[uf].insert(vf as usize);
        }
    }
    contained
}

fn record_import_edges<'a>(
    ingest: &'a IngestResult,
    interner: &'a Interner,
    file_index: &std::collections::HashMap<&str, u32>,
    adj_sets: &mut [std::collections::BTreeSet<usize>],
    inc_targets: &mut [Vec<&'a str>],
) {
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let f_len = ingest.files.len();
    for r in &ingest.references {
        let uf = r.file_id as usize;
        if r.role != crate::domain::RefRole::Import || uf >= f_len {
            continue;
        }
        let raw = interner.resolve(&r.callee_name);
        let clean = crate::ingest::strip_import_quotes(raw);
        if !clean.is_empty() && !inc_targets[uf].contains(&clean) {
            inc_targets[uf].push(clean);
        }
        let f_path = view.file_path(r.file_id);
        if let Some(vf) = crate::languages::resolve_import(
            ingest.files[uf].lang,
            f_path,
            raw,
            file_index,
            &ingest.import_aliases,
        ) && (vf as usize) != uf
            && (vf as usize) < f_len
        {
            adj_sets[uf].insert(vf as usize);
        }
    }
}

/// The file graph `--deps` measures: dependency edges per file, the import texts each file names, and the
/// containment edges (`mod x;`) that are not dependencies.
struct FileAdjacency<'a> {
    dependency: Vec<Vec<usize>>,
    inc_targets: Vec<Vec<&'a str>>,
    contained: Vec<Vec<usize>>,
}

fn collect_import_and_call_adj<'a>(
    ingest: &'a IngestResult,
    graph: &Graph,
    interner: &'a Interner,
) -> FileAdjacency<'a> {
    let f_len = ingest.files.len();
    let file_index: std::collections::HashMap<&str, u32> = ingest
        .files
        .iter()
        .map(|f| (interner.resolve(&f.path), f.id))
        .collect();
    let mut adj_sets = vec![std::collections::BTreeSet::new(); f_len];
    let mut inc_targets = vec![Vec::new(); f_len];
    record_import_edges(
        ingest,
        interner,
        &file_index,
        &mut adj_sets,
        &mut inc_targets,
    );

    let has_import_edges = adj_sets.iter().any(|s| !s.is_empty());
    if !has_import_edges {
        for u in 0..graph.node_count() as NodeId {
            let uf = ingest.symbols[u as usize].file_id as usize;
            for edge in graph.callee_edges(u) {
                let vf = ingest.symbols[edge.to as usize].file_id as usize;
                if !edge.prov.is_split() && uf != vf && uf < f_len && vf < f_len {
                    adj_sets[uf].insert(vf);
                }
            }
        }
    }

    let contained = collect_containment_edges(ingest, interner, &file_index)
        .into_iter()
        .zip(&adj_sets)
        .map(|(declared, depends)| declared.difference(depends).copied().collect())
        .collect();
    let adj = adj_sets
        .into_iter()
        .map(|s| s.into_iter().collect())
        .collect();
    FileAdjacency {
        dependency: adj,
        inc_targets,
        contained,
    }
}

struct TarjanState {
    idx: Vec<i32>,
    low: Vec<i32>,
    on_stack: Vec<bool>,
    stack: Vec<usize>,
    counter: i32,
    cycles: Vec<Vec<usize>>,
}

fn tarjan_dfs(u: usize, adj: &[Vec<usize>], st: &mut TarjanState) {
    st.idx[u] = st.counter;
    st.low[u] = st.counter;
    st.counter += 1;
    st.stack.push(u);
    st.on_stack[u] = true;

    for &w in &adj[u] {
        if st.idx[w] == -1 {
            tarjan_dfs(w, adj, st);
            st.low[u] = st.low[u].min(st.low[w]);
        } else if st.on_stack[w] {
            st.low[u] = st.low[u].min(st.idx[w]);
        }
    }

    if st.low[u] == st.idx[u] {
        let mut comp = Vec::new();
        while let Some(w) = st.stack.pop() {
            st.on_stack[w] = false;
            comp.push(w);
            if w == u {
                break;
            }
        }
        if comp.len() > 1 {
            comp.reverse();
            st.cycles.push(comp);
        }
    }
}

pub fn find_scc_cycles(adj: &[Vec<usize>]) -> Vec<Vec<usize>> {
    let n = adj.len();
    let mut st = TarjanState {
        idx: vec![-1; n],
        low: vec![0; n],
        on_stack: vec![false; n],
        stack: Vec::new(),
        counter: 0,
        cycles: Vec::new(),
    };
    for i in 0..n {
        if st.idx[i] == -1 {
            tarjan_dfs(i, adj, &mut st);
        }
    }
    st.cycles
}

fn compute_transitive_closure_counts(adj: &[Vec<usize>]) -> Vec<usize> {
    let n = adj.len();
    let mut counts = vec![1usize; n];
    let mut seen = vec![0usize; n];
    let mut stack = Vec::new();
    for s in 0..n {
        let epoch = s + 1;
        seen[s] = epoch;
        stack.clear();
        stack.push(s);
        let mut reached = 0usize;
        while let Some(u) = stack.pop() {
            reached += 1;
            for &v in &adj[u] {
                if seen[v] != epoch {
                    seen[v] = epoch;
                    stack.push(v);
                }
            }
        }
        counts[s] = reached;
    }
    counts
}

fn collect_stabledeps_violations<'a>(
    ingest: &'a IngestResult,
    interner: &'a Interner,
    adj: &[Vec<usize>],
    afferent: &[usize],
) -> Vec<(&'a str, &'a str, f64)> {
    let instab_of = |f: usize| -> f64 {
        let ce = adj[f].len() as f64;
        let ca = afferent[f] as f64;
        if ca + ce > 0.0 { ce / (ca + ce) } else { 0.0 }
    };
    let mut violations = Vec::new();
    for (u, neighbors) in adj.iter().enumerate() {
        let iu = instab_of(u);
        let pu = interner.resolve(&ingest.files[u].path);
        for &v in neighbors {
            let gap = instab_of(v) - iu;
            if gap > 0.05 {
                violations.push((pu, interner.resolve(&ingest.files[v].path), gap));
            }
        }
    }
    violations.sort_by(|a, b| {
        b.2.partial_cmp(&a.2)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.0.cmp(b.0))
    });
    violations
}

fn afferent_counts(adj: &[Vec<usize>], f_len: usize) -> Vec<usize> {
    let mut afferent = vec![0usize; f_len];
    for &v in adj.iter().flatten() {
        afferent[v] += 1;
    }
    afferent
}

fn cycle_paths<'a>(
    ingest: &'a IngestResult,
    interner: &'a Interner,
    adj: &[Vec<usize>],
) -> Vec<Vec<&'a str>> {
    find_scc_cycles(adj)
        .into_iter()
        .map(|c| {
            c.into_iter()
                .map(|fid| interner.resolve(&ingest.files[fid].path))
                .collect()
        })
        .collect()
}

/// Afferent counts with each containment edge added to the file it declares.
fn with_containment(dependent: &[usize], contained: &[Vec<usize>]) -> Vec<usize> {
    let mut afferent = dependent.to_vec();
    for &v in contained.iter().flatten() {
        afferent[v] += 1;
    }
    afferent
}

pub fn compute_deps_report<'a>(
    ingest: &'a IngestResult,
    graph: &Graph,
    interner: &'a Interner,
) -> crate::serialize::DepsReport<'a> {
    let f_len = ingest.files.len();
    let FileAdjacency {
        dependency: adj,
        inc_targets,
        contained,
    } = collect_import_and_call_adj(ingest, graph, interner);
    let dependent = afferent_counts(&adj, f_len);
    let afferent = with_containment(&dependent, &contained);
    let transitive = compute_transitive_closure_counts(&adj);
    let cycles = cycle_paths(ingest, interner, &adj);
    let violations = collect_stabledeps_violations(ingest, interner, &adj, &dependent);

    let mut rows = Vec::with_capacity(f_len);
    let mut ccd = 0usize;
    let mut dep_files = 0usize;
    for (i, f) in ingest.files.iter().enumerate() {
        if f.lang.is_dependency_capable() {
            dep_files += 1;
            ccd += transitive[i];
        }
        let linked = adj[i].len() + contained[i].len();
        rows.push(crate::serialize::DepsFileRow {
            path: interner.resolve(&f.path),
            includes: inc_targets[i].len().max(linked),
            efferent: linked,
            afferent: afferent[i],
            transitive: transitive[i],
            inc_targets: inc_targets[i].clone(),
        });
    }
    rows.sort_by(|a, b| {
        (b.transitive + b.afferent)
            .cmp(&(a.transitive + a.afferent))
            .then_with(|| a.path.cmp(b.path))
    });
    let n = dep_files.max(1) as f64;
    let acd = ccd as f64 / n;
    let btree = ((n + 1.0) * (n + 1.0).log2() - n).max(1.0);
    crate::serialize::DepsReport {
        corpus_files: f_len,
        dep_files,
        ccd,
        acd,
        nccd: ccd as f64 / btree,
        rows,
        cycles,
        violations,
        containment_edges: contained.iter().map(Vec::len).sum(),
        page: crate::serialize::PageRequest::default(),
    }
}
