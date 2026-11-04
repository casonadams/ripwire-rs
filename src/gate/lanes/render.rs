use super::LaneCorpus;
use super::lane::BuiltLane;
use super::pairs::compute_pairs_and_landing_order;
use crate::domain::{IngestResult, Interner, NodeId};
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub(super) fn build_core_json(
    core_ids: &[NodeId],
    ingest: &IngestResult,
    interner: &Interner,
) -> Value {
    let mut files = BTreeSet::new();
    let mut symbols = Vec::new();
    for &id in core_ids {
        if let Some(sym) = ingest.sym(id) {
            let p = ingest.sym_file_path(sym, interner).to_string();
            let n = interner.resolve(&sym.name).to_string();
            let scope = sym.scope.map_or("", |sc| interner.resolve(&sc)).to_string();
            files.insert(p.clone());
            symbols.push(json!({
                "p": p,
                "n": n,
                "scope": scope,
                "l": sym.line,
                "id": if scope.is_empty() { Value::Null } else { json!(format!("{scope}::{n}")) },
            }));
        }
    }
    json!({
        "files": files.into_iter().collect::<Vec<_>>(),
        "symbols": symbols,
    })
}

pub(super) fn assemble_plan_lanes_envelope(
    root: &str,
    task_val: Value,
    source: &str,
    requested: usize,
    carve_val: Value,
    core_val: Value,
    mut built_lanes: Vec<BuiltLane>,
) -> Value {
    let at_stamp = crate::git::history::compute_git_at_stamp(root).map_or_else(
        |reason| json!({ "unavailable": reason }),
        |stamp| json!(stamp),
    );
    let (pairs, landing_order) = compute_pairs_and_landing_order(&mut built_lanes);
    let lanes_json: Vec<Value> = built_lanes.into_iter().map(|l| l.json_val).collect();
    json!({
        "v": 1,
        "verb": "plan-lanes",
        "at": at_stamp,
        "root": root,
        "task": task_val,
        "source": source,
        "requested": requested,
        "lane_count": lanes_json.len(),
        "claim_key": "path+scope+name",
        "on_conflict": "producing-lane-rebases",
        "carve": carve_val,
        "core": core_val,
        "lanes": lanes_json,
        "pairs": pairs,
        "landing_order": landing_order,
        "landing_rule": "fewest-conflicts-first greedy; ties by lane id ascending (lexicographic on the lane id string)",
        "contract_touch_rule": "a claim of lane `from` lies in the transitive-caller blast radius of lane `to`'s claims, so `from` may have to adapt to a contract change `to` makes; it is NOT a merge conflict and is never counted into conflict_count",
        "warnings": [],
    })
}

pub(super) fn attach_corpus_stats(mut val: Value, corpus: LaneCorpus<'_>) -> Value {
    let gauges = corpus.graph.gauges();
    val["corpus"] = json!({
        "files": corpus.ingest.files.len(),
        "symbols": corpus.ingest.symbol_count(),
        "edges": corpus.graph.edge_count(),
        "ambiguous": gauges.ambiguous,
        "unresolved": gauges.unresolved,
    });
    if let Err(reason) = corpus.churn {
        val["churn_unavailable"] = json!(reason);
    }
    val
}
