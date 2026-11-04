use super::claims::{LaneClaimsSummary, build_lane_claims_and_files};
use super::env::{LaneBuildEnv, ambiguity_of};
use super::reach::{LaneBlast, LaneTests, lane_blast, lane_model, lane_tests};
use crate::domain::NodeId;
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub(super) struct BuiltLane {
    pub(super) id: String,
    pub(super) lane_num: usize,
    pub(super) node_ids: Vec<NodeId>,
    pub(super) claim_keys: BTreeMap<u64, (String, String, NodeId)>,
    pub(super) file_counts: BTreeMap<String, usize>,
    pub(super) blast_ids: BTreeSet<NodeId>,
    pub(super) json_val: Value,
}

/// The inputs a lane's JSON reports, gathered once by `build_single_lane`.
struct LaneReport<'a> {
    lane_id: &'a str,
    lane_idx: usize,
    lane_task: &'a str,
    module_span: usize,
    symbol_count: usize,
    ambiguous_calls: usize,
    model: &'static str,
    claims: &'a LaneClaimsSummary,
    blast: &'a LaneBlast,
    tests: &'a LaneTests,
}

fn lane_json(report: &LaneReport<'_>) -> Value {
    let (claims, blast, tests) = (report.claims, report.blast, report.tests);
    let module_span = report.module_span.max(1);
    json!({
        "id": report.lane_id,
        "lane": report.lane_idx + 1,
        "target": format!("worktree-lane-{}", report.lane_idx + 1),
        "task": report.lane_task,
        "symbols_count": claims.sym_rows.len(),
        "symbols": claims.sym_rows,
        "claims": {
            "symbols": claims.sym_rows,
            "files": claims.file_rows,
        },
        "blast_radius": {
            "reaches": blast.reaches,
            "files_total": blast.files_total,
            "capped": blast.capped,
            "files": blast.files,
        },
        "tests_to_run": tests.to_run,
        "tests_total": tests.total,
        "tests_capped": tests.capped,
        "tests_granularity": "claimed-symbols",
        "untested": claims.untested,
        "module_span": module_span,
        "notes": [],
        "execution": {
            "policy": "isolated-worktree",
            "model": report.model,
            "reasoning": if report.model == "frontier" { "high" } else { "medium" },
            "rule": "ccx-and-blast-threshold",
            "basis": "static-call-graph",
            "signals": {
                "claims": report.symbol_count,
                "files": claims.file_counts.len(),
                "module_span": module_span,
                "max_ccx": claims.max_ccx,
                "sum_ccx": claims.sum_ccx,
                "ambiguous_calls": report.ambiguous_calls,
                "blast_reaches": blast.reaches,
                "untested": claims.untested,
                "tests_total": tests.total,
                "blast_capped": blast.capped,
                "tests_capped": tests.capped,
            },
            "caveats": [],
        }
    })
}

pub(super) fn build_single_lane(
    lane_idx: usize,
    lane_task: &str,
    node_ids: Vec<NodeId>,
    module_span: usize,
    env: &LaneBuildEnv<'_>,
) -> BuiltLane {
    let lane_id = format!("lane-{lane_idx}");
    let claims = build_lane_claims_and_files(&node_ids, env);
    let blast = lane_blast(&node_ids, env);
    let tests = lane_tests(&claims.file_counts, env);
    let json_val = lane_json(&LaneReport {
        lane_id: &lane_id,
        lane_idx,
        lane_task,
        module_span,
        symbol_count: node_ids.len(),
        ambiguous_calls: node_ids.iter().map(|&id| ambiguity_of(env.graph, id)).sum(),
        model: lane_model(claims.max_ccx, blast.reaches),
        claims: &claims,
        blast: &blast,
        tests: &tests,
    });
    BuiltLane {
        id: lane_id,
        lane_num: lane_idx + 1,
        node_ids,
        claim_keys: claims.claim_keys,
        file_counts: claims.file_counts,
        blast_ids: blast.ids,
        json_val,
    }
}
