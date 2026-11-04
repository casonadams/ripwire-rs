use super::LaneCorpus;
use crate::domain::{IngestResult, Interner, NodeId, SymKind};
use crate::graph::Graph;
use crate::serialize::PageRequest;
use std::collections::HashMap;

/// Corpus-wide tables every lane reads: test reach, file hotspot ranks, and definitions per `(scope, name)`.
pub(super) struct LaneIndex {
    pub(super) tested_mask: Vec<bool>,
    pub(super) hot_ranks: HashMap<String, usize>,
    pub(super) id_defs: HashMap<(String, String), usize>,
}

pub(super) fn lane_index(corpus: LaneCorpus<'_>) -> LaneIndex {
    let mut id_defs: HashMap<(String, String), usize> = HashMap::new();
    for sym in corpus
        .ingest
        .symbols
        .iter()
        .filter(|s| s.kind != SymKind::Section && s.kind != SymKind::ModuleScope)
    {
        let scope = sym
            .scope
            .map_or("", |sc| corpus.interner.resolve(&sc))
            .to_string();
        let name = corpus.interner.resolve(&sym.name).to_string();
        *id_defs.entry((scope, name)).or_insert(0) += 1;
    }
    let hot_ranks = corpus.churn.ok().map_or_else(HashMap::new, |churn| {
        crate::metrics::compute_hotspots(corpus.ingest, corpus.interner, churn)
            .files
            .iter()
            .enumerate()
            .map(|(i, f)| (f.file_path.clone(), i + 1))
            .collect()
    });
    LaneIndex {
        tested_mask: crate::navigate::compute_tested_node_mask(
            corpus.ingest,
            corpus.graph,
            corpus.interner,
        ),
        hot_ranks,
        id_defs,
    }
}

pub(super) struct LaneBuildEnv<'a> {
    pub(super) ingest: &'a IngestResult,
    pub(super) graph: &'a Graph,
    pub(super) ranks: &'a [f64],
    pub(super) interner: &'a Interner,
    pub(super) churn: Option<&'a HashMap<String, u32>>,
    pub(super) index: &'a LaneIndex,
    pub(super) page: PageRequest,
}

pub(super) fn lane_env<'a>(
    corpus: LaneCorpus<'a>,
    index: &'a LaneIndex,
    page: PageRequest,
) -> LaneBuildEnv<'a> {
    LaneBuildEnv {
        ingest: corpus.ingest,
        graph: corpus.graph,
        ranks: &corpus.pr.ranks,
        interner: corpus.interner,
        churn: corpus.churn.ok(),
        index,
        page,
    }
}

pub(super) fn ambiguity_of(graph: &Graph, id: NodeId) -> usize {
    graph.amb_out.get(id as usize).map_or(0, |&n| n as usize)
}
