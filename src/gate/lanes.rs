mod claims;
mod env;
mod lane;
mod pairs;
mod partition;
mod reach;
mod render;

pub use partition::{OverlapStats, PartitionPlan, measure_partition_overlap, plan_task_partition};

use self::env::{lane_env, lane_index};
use self::lane::build_single_lane;
use self::partition::{group_assignable_by_community, rank_task_surface_nodes};
use self::render::{assemble_plan_lanes_envelope, attach_corpus_stats, build_core_json};
use crate::domain::{IngestResult, Interner, NodeId};
use crate::graph::{Graph, community::Communities};
use crate::rank::PageRankResult;
use crate::serialize::PageRequest;
use serde_json::{Value, json};
use std::collections::{BTreeSet, HashMap};

/// The indexed corpus a plan-lanes run reads: the root for the envelope, the index, PageRank ranks, and the
/// per-file commit counts over the git history window (`Err` with the reason when that history cannot be read).
#[derive(Clone, Copy)]
pub struct LaneCorpus<'a> {
    pub root: &'a str,
    pub ingest: &'a IngestResult,
    pub graph: &'a Graph,
    pub pr: &'a PageRankResult,
    pub interner: &'a Interner,
    pub churn: Result<&'a HashMap<String, u32>, &'a str>,
}

#[must_use]
pub fn compute_plan_lanes_with_brief(
    corpus: LaneCorpus<'_>,
    brief_lines: &[String],
    page: PageRequest,
) -> Value {
    let index = lane_index(corpus);
    let env = lane_env(corpus, &index, page);
    let communities: Communities = corpus.graph.communities();
    let non_blank: Vec<&str> = brief_lines
        .iter()
        .map(String::as_str)
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    let mut built_lanes = Vec::new();
    for (idx, &line_task) in non_blank.iter().enumerate() {
        let nodes = rank_task_surface_nodes(
            line_task,
            15,
            corpus.ingest,
            &corpus.pr.ranks,
            corpus.interner,
        );
        let module_span =
            group_assignable_by_community(&nodes, corpus.ingest, corpus.graph, &communities).len();
        built_lanes.push(build_single_lane(idx, line_task, nodes, module_span, &env));
    }
    let base = assemble_plan_lanes_envelope(
        corpus.root,
        Value::Null,
        "brief",
        non_blank.len(),
        Value::Null,
        json!({ "files": [], "symbols": [] }),
        built_lanes,
    );
    attach_corpus_stats(base, corpus)
}

#[must_use]
pub fn compute_plan_lanes(
    corpus: LaneCorpus<'_>,
    requested: usize,
    task: &str,
    page: PageRequest,
) -> Value {
    let index = lane_index(corpus);
    let env = lane_env(corpus, &index, page);
    let n = requested.clamp(1, 16);
    let plan = plan_task_partition(
        task,
        n,
        corpus.ingest,
        corpus.graph,
        &corpus.pr.ranks,
        corpus.interner,
    );
    let mut built_lanes = Vec::new();
    for (idx, grp) in plan.groups.iter().enumerate() {
        let mod_span = plan.group_module_count.get(idx).copied().unwrap_or(1);
        built_lanes.push(build_single_lane(idx, task, grp.clone(), mod_span, &env));
    }
    let lane_surfaces: Vec<BTreeSet<NodeId>> = built_lanes
        .iter()
        .map(|l| {
            let mut s: BTreeSet<NodeId> = l.node_ids.iter().copied().collect();
            s.extend(&l.blast_ids);
            s
        })
        .collect();
    let core_surface: BTreeSet<NodeId> = plan.core_ids.iter().copied().collect();
    let ov = measure_partition_overlap(&lane_surfaces, &core_surface);
    let carve_val = json!({
        "strategy": "leiden-community",
        "surface": plan.surface_count,
        "modules": plan.module_count,
        "split": plan.split_count,
        "overlap_mean": (ov.mean * 1000.0).round() / 1000.0,
        "overlap_max": (ov.worst * 1000.0).round() / 1000.0,
        "shared_symbols": ov.shared_count,
        "union_symbols": ov.union_count,
        "core_overlap": (ov.core_leak * 1000.0).round() / 1000.0,
        "overlap_surface": "claims-plus-blast-radius",
        "overlap_is_ceiling": true,
    });
    let core_val = build_core_json(&plan.core_ids, corpus.ingest, corpus.interner);
    let base = assemble_plan_lanes_envelope(
        corpus.root,
        json!(task),
        "partition",
        n,
        carve_val,
        core_val,
        built_lanes,
    );
    attach_corpus_stats(base, corpus)
}
