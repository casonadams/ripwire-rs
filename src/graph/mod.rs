pub mod community;
pub mod csr;
pub mod mermaid;
pub mod value_reach;
pub mod zoom;

pub use csr::CsrMatrix;
pub use mermaid::generate_mermaid_diagram;

use crate::domain::{IngestResult, Interner, NodeId};
use crate::rank::{PageRankConfig, PageRankResult, page_rank, uniform_teleport};
use crate::resolve::{EdgeProv, ResolutionResult, resolve_all_references};
use csr::build_in_edges_csr;
use serde::{Deserialize, Serialize};

/// One outgoing call edge with its resolution provenance.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CalleeEdge {
    pub to: NodeId,
    pub weight: f32,
    pub prov: EdgeProv,
}

/// The resolved repository call graph.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Graph {
    pub in_edges: CsrMatrix,
    pub w_out_deg: Vec<f64>,
    pub out_offsets: Vec<u32>,
    pub out_targets: Vec<NodeId>,
    pub out_weights: Vec<f32>,
    pub out_prov: Vec<EdgeProv>,
    pub amb_out: Vec<u32>,
    #[serde(default)]
    pub unresolved: usize,
    #[serde(default)]
    pub external: usize,
    #[serde(default)]
    pub declined: usize,
    #[serde(default)]
    pub unindexed: usize,
    #[serde(default)]
    pub prior_weights: Vec<f64>,
}

impl Graph {
    #[must_use]
    pub fn node_count(&self) -> usize {
        self.in_edges.rows
    }

    #[must_use]
    pub fn edge_count(&self) -> usize {
        self.in_edges.nnz()
    }

    #[must_use]
    pub fn is_dangling(&self, node: NodeId) -> bool {
        self.w_out_deg[node as usize] == 0.0
    }

    #[must_use]
    pub fn out_degree(&self, node: NodeId) -> usize {
        let n = node as usize;
        (self.out_offsets[n + 1] - self.out_offsets[n]) as usize
    }

    #[must_use]
    pub fn in_degree(&self, node: NodeId) -> usize {
        let n = node as usize;
        (self.in_edges.row_offsets[n + 1] - self.in_edges.row_offsets[n]) as usize
    }

    pub fn callers(&self, node: NodeId) -> impl Iterator<Item = (NodeId, f32)> + '_ {
        self.in_edges.row_entries(node as usize)
    }

    pub fn callees(&self, node: NodeId) -> impl Iterator<Item = (NodeId, f32)> + '_ {
        self.callee_edges(node).map(|e| (e.to, e.weight))
    }

    /// Outgoing edges that carry call flow. Type-only edges (weight 0) stay navigable through `callee_edges`
    /// but are not calls, so ranked call views use this.
    pub fn call_flow_edges(&self, node: NodeId) -> impl Iterator<Item = CalleeEdge> + '_ {
        self.callee_edges(node).filter(|e| e.weight > 0.0)
    }
    pub fn callee_edges(&self, node: NodeId) -> impl Iterator<Item = CalleeEdge> + '_ {
        let n = node as usize;
        let start = self.out_offsets[n] as usize;
        let end = self.out_offsets[n + 1] as usize;
        (start..end).map(move |i| CalleeEdge {
            to: self.out_targets[i],
            weight: self.out_weights[i],
            prov: self.out_prov.get(i).copied().unwrap_or(EdgeProv::Local),
        })
    }

    /// Provenance of the `from -> to` edge, if the graph has one.
    #[must_use]
    pub fn edge_prov(&self, from: NodeId, to: NodeId) -> Option<EdgeProv> {
        self.callee_edges(from).find(|e| e.to == to).map(|e| e.prov)
    }

    #[must_use]
    pub fn ambiguity(&self, node: NodeId) -> u32 {
        self.amb_out[node as usize]
    }

    #[must_use]
    pub fn total_ambiguous(&self) -> usize {
        self.amb_out.iter().map(|&x| x as usize).sum()
    }

    /// Call edges a SCIP index pinned.
    #[must_use]
    pub fn scip_edge_count(&self) -> usize {
        self.out_prov
            .iter()
            .filter(|&&p| p == EdgeProv::Scip)
            .count()
    }

    #[must_use]
    pub fn total_unresolved(&self) -> usize {
        self.unresolved
    }

    #[must_use]
    pub fn total_unindexed(&self) -> usize {
        self.unindexed
    }

    #[must_use]
    pub fn total_external(&self) -> usize {
        self.external
    }

    #[must_use]
    pub fn total_declined(&self) -> usize {
        self.declined
    }

    #[must_use]
    pub fn gauges(&self) -> crate::serialize::GraphGauges {
        crate::serialize::GraphGauges {
            ambiguous: self.total_ambiguous(),
            unresolved: self.total_unresolved(),
            external: self.total_external(),
            declined: self.total_declined(),
            unindexed: self.total_unindexed(),
        }
    }

    #[must_use]
    pub fn xml_gauge_attrs(&self) -> String {
        crate::serialize::GraphGaugeProvider::xml_gauge_attrs(self)
    }

    #[must_use]
    pub fn page_rank(&self, config: &PageRankConfig) -> PageRankResult {
        let teleport =
            if self.prior_weights.len() == self.node_count() && !self.prior_weights.is_empty() {
                crate::rank::biased_teleport(&self.prior_weights)
            } else {
                uniform_teleport(self.node_count())
            };
        self.page_rank_custom(config, &teleport)
    }

    #[must_use]
    pub fn page_rank_custom(&self, config: &PageRankConfig, teleport: &[f64]) -> PageRankResult {
        page_rank(&self.in_edges, &self.w_out_deg, teleport, config)
    }

    #[must_use]
    pub fn communities(&self) -> community::Communities {
        community::detect_communities(self)
    }

    #[must_use]
    /// HITS with callers counted per file; `node_files[node]` is each node's file.
    pub fn hits(&self, node_files: &[u32]) -> crate::rank::HitsResult {
        crate::rank::compute_hits_by_file(self, node_files, &crate::rank::HitsConfig::default())
    }

    #[must_use]
    pub fn build(ingest: &IngestResult, interner: &Interner) -> Self {
        let resolution = resolve_all_references(ingest, interner);
        let mut sym_files = vec![false; ingest.files.len()];
        for s in &ingest.symbols {
            if let Some(slot) = sym_files.get_mut(s.file_id as usize) {
                *slot = true;
            }
        }
        let unindexed = ingest
            .files
            .iter()
            .enumerate()
            .filter(|&(i, f)| {
                !f.lang.semantics().is_prose_document()
                    && !sym_files.get(i).copied().unwrap_or(false)
            })
            .count();
        let mut g = Self::from_resolution(ingest.symbols.len(), resolution);
        g.unindexed = unindexed;
        g.prior_weights = compute_prior_weights(ingest, interner);
        g
    }

    #[must_use]
    pub fn from_resolution(node_count: usize, resolution: ResolutionResult) -> Self {
        let mut edges: Vec<(NodeId, NodeId, f32, EdgeProv)> = resolution
            .edge_accumulators
            .into_iter()
            .map(|((from, to), acc)| {
                // Type-only edges stay navigable (callers, uses) but carry no call-flow mass.
                let w = if acc.calls == 0 {
                    0.0
                } else {
                    ((acc.conf_sum / acc.n_ref as f32) * (acc.n_ref as f32).sqrt()).min(8.0)
                };
                (from, to, w, acc.prov)
            })
            .collect();

        edges.sort_by_key(|&(from, to, _, _)| (from, to));

        let out = build_out_edges(node_count, &edges);
        let in_edges_tuples: Vec<(NodeId, NodeId, f32)> =
            edges.iter().map(|&(f, t, w, _)| (f, t, w)).collect();
        let in_edges = build_in_edges_csr(node_count, &in_edges_tuples);

        Self {
            in_edges,
            w_out_deg: out.w_out_deg,
            out_offsets: out.offsets,
            out_targets: out.targets,
            out_weights: out.weights,
            out_prov: out.prov,
            amb_out: resolution.amb_out,
            unresolved: resolution.unresolved,
            external: resolution.external,
            declined: resolution.declined,
            unindexed: 0,
            prior_weights: Vec::new(),
        }
    }
}

impl crate::serialize::GraphGaugeProvider for Graph {
    #[inline]
    fn gauges(&self) -> crate::serialize::GraphGauges {
        self.gauges()
    }
}

fn identifier_word_count(id: &str) -> usize {
    let mut words = 0usize;
    let mut in_word = false;
    let mut prev = 0u8;
    for &b in id.as_bytes() {
        if !b.is_ascii_alphanumeric() {
            in_word = false;
            prev = b;
            continue;
        }
        let camel_boundary = b.is_ascii_uppercase() && in_word && !prev.is_ascii_uppercase();
        if !in_word || camel_boundary {
            words += 1;
        }
        in_word = true;
        prev = b;
    }
    words
}

fn symbol_prior_weight(
    view: &(&IngestResult, &Interner),
    s: &crate::domain::Symbol,
    def_cnt: usize,
    has_prod: bool,
) -> f64 {
    use crate::domain::CorpusView;
    let name = view.sym_name(s);
    let mut w = 1.0f64;
    if def_cnt > 5 {
        w *= 0.5;
    }
    if name.starts_with('_') {
        w *= 0.5;
    }
    if name.len() >= 8 && identifier_word_count(name) >= 2 {
        w *= 1.7;
    }
    if s.lang.semantics().is_std_or_builtin_member(name) {
        w *= 0.25;
    }
    if matches!(
        s.kind,
        crate::domain::SymKind::Field | crate::domain::SymKind::Var
    ) {
        w *= 0.4;
    }
    if has_prod && !view.is_production_symbol(s) {
        w *= 0.05;
    }
    w
}

fn compute_prior_weights(ingest: &IngestResult, interner: &Interner) -> Vec<f64> {
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let mut name_counts: std::collections::HashMap<lasso::Spur, usize> =
        std::collections::HashMap::with_capacity(ingest.symbols.len());
    for s in &ingest.symbols {
        *name_counts.entry(s.name).or_default() += 1;
    }
    let has_prod = ingest.symbols.iter().any(|s| view.is_production_symbol(s));
    ingest
        .symbols
        .iter()
        .map(|s| {
            let def_cnt = name_counts.get(&s.name).copied().unwrap_or(0);
            symbol_prior_weight(&view, s, def_cnt, has_prod)
        })
        .collect()
}

struct OutEdges {
    offsets: Vec<u32>,
    targets: Vec<NodeId>,
    weights: Vec<f32>,
    prov: Vec<EdgeProv>,
    w_out_deg: Vec<f64>,
}

fn build_out_edges(node_count: usize, edges: &[(NodeId, NodeId, f32, EdgeProv)]) -> OutEdges {
    let mut out_offsets = vec![0u32; node_count + 1];
    for &(from, _, _, _) in edges {
        out_offsets[from as usize + 1] += 1;
    }
    for i in 0..node_count {
        out_offsets[i + 1] += out_offsets[i];
    }

    let mut out_targets = vec![0u32; edges.len()];
    let mut out_weights = vec![0.0f32; edges.len()];
    let mut out_prov = vec![EdgeProv::Local; edges.len()];
    let mut w_out_deg = vec![0.0f64; node_count];
    let mut cur = out_offsets[0..node_count].to_vec();

    for &(from, to, w, prov) in edges {
        let pos = cur[from as usize] as usize;
        cur[from as usize] += 1;
        out_targets[pos] = to;
        out_weights[pos] = w;
        out_prov[pos] = prov;
        w_out_deg[from as usize] += w as f64;
    }

    OutEdges {
        offsets: out_offsets,
        targets: out_targets,
        weights: out_weights,
        prov: out_prov,
        w_out_deg,
    }
}
