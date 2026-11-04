use super::env::LaneBuildEnv;
use crate::domain::NodeId;
use crate::serialize::{Paged, text_bytes};
use std::collections::{BTreeMap, BTreeSet};

/// What a lane reaches through the call graph: its blast radius, and the files it touches there.
pub(super) struct LaneBlast {
    pub(super) reaches: usize,
    pub(super) files_total: usize,
    pub(super) capped: bool,
    pub(super) files: Vec<String>,
    pub(super) ids: BTreeSet<NodeId>,
}

pub(super) fn lane_blast(node_ids: &[NodeId], env: &LaneBuildEnv<'_>) -> LaneBlast {
    let impact = crate::navigate::transitive_impact_multi(
        node_ids,
        env.ingest,
        env.graph,
        env.ranks,
        env.interner,
        None,
    );
    let ids: BTreeSet<NodeId> = impact.iter().map(|n| n.id).collect();
    let files_set: BTreeSet<String> = impact.iter().map(|n| n.path.clone()).collect();
    let all: Vec<&String> = files_set.iter().collect();
    let paged = Paged::new(&all, env.page, |p| text_bytes(&[p.as_str()]));
    LaneBlast {
        reaches: impact.len(),
        files_total: files_set.len(),
        capped: paged.window.capped,
        files: paged.rows().iter().map(|path| (*path).clone()).collect(),
        ids,
    }
}

/// The affected tests a lane runs: those reached from the files its claims touch.
pub(super) struct LaneTests {
    pub(super) total: usize,
    pub(super) capped: bool,
    pub(super) to_run: Vec<String>,
}

pub(super) fn lane_tests(
    file_counts: &BTreeMap<String, usize>,
    env: &LaneBuildEnv<'_>,
) -> LaneTests {
    let changed_paths: Vec<&str> = file_counts.keys().map(String::as_str).collect();
    let aff = crate::gate::affected::find_affected_tests(
        &changed_paths,
        env.ingest,
        env.graph,
        env.interner,
    );
    let paged = Paged::new(&aff.tests, env.page, |t| text_bytes(&[t.path.as_str()]));
    LaneTests {
        total: aff.tests.len(),
        capped: paged.window.capped,
        to_run: paged.rows().iter().map(|t| t.path.clone()).collect(),
    }
}

pub(super) fn lane_model(max_ccx: u32, reaches: usize) -> &'static str {
    if max_ccx >= 15 || reaches > 20 {
        "frontier"
    } else {
        "standard"
    }
}
