use crate::domain::{IngestResult, Interner, NodeId};
use crate::graph::{Graph, community::Communities};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, Clone, Default)]
pub struct PartitionPlan {
    pub core_ids: Vec<NodeId>,
    pub groups: Vec<Vec<NodeId>>,
    pub group_module_count: Vec<usize>,
    pub surface_count: usize,
    pub module_count: usize,
    pub split_count: usize,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct OverlapStats {
    pub mean: f64,
    pub worst: f64,
    pub core_leak: f64,
    pub shared_count: usize,
    pub union_count: usize,
}

fn group_key_for_node(id: NodeId, ingest: &IngestResult, graph: &Graph, cm: &Communities) -> u64 {
    if graph.out_degree(id) > 0 || graph.in_degree(id) > 0 {
        u64::from(cm.comm.get(id as usize).copied().unwrap_or(0))
    } else {
        let file_id = ingest.sym(id).map_or(0, |s| s.file_id);
        (1u64 << 32) | u64::from(file_id)
    }
}

pub(super) fn group_assignable_by_community(
    assignable: &[NodeId],
    ingest: &IngestResult,
    graph: &Graph,
    cm: &Communities,
) -> Vec<Vec<NodeId>> {
    let mut slot_of_key: BTreeMap<u64, usize> = BTreeMap::new();
    let mut members: Vec<Vec<NodeId>> = Vec::new();
    for &id in assignable {
        let key = group_key_for_node(id, ingest, graph, cm);
        let slot = *slot_of_key.entry(key).or_insert_with(|| {
            let idx = members.len();
            members.push(Vec::new());
            idx
        });
        members[slot].push(id);
    }
    members
}

fn widest_splittable_group(groups: &[Vec<NodeId>]) -> Option<usize> {
    let mut best: Option<usize> = None;
    for (idx, grp) in groups.iter().enumerate() {
        if grp.len() >= 2 && best.is_none_or(|b| grp.len() > groups[b].len()) {
            best = Some(idx);
        }
    }
    best
}

fn split_groups_up_to(groups: &mut Vec<Vec<NodeId>>, target_count: usize) -> usize {
    let mut cuts = 0usize;
    while groups.len() < target_count {
        let Some(widest) = widest_splittable_group(groups) else {
            break;
        };
        let half = groups[widest].len().div_ceil(2);
        let tail = groups[widest].split_off(half);
        groups.push(tail);
        cuts += 1;
    }
    cuts
}

fn pack_groups_into_bins(
    groups: &[Vec<NodeId>],
    bin_count: usize,
    rank_pos: &BTreeMap<NodeId, usize>,
) -> (Vec<Vec<NodeId>>, Vec<usize>) {
    let mut by_weight: Vec<usize> = (0..groups.len()).collect();
    by_weight.sort_by(|&a, &b| {
        groups[b]
            .len()
            .cmp(&groups[a].len())
            .then_with(|| a.cmp(&b))
    });
    let mut bins = vec![Vec::new(); bin_count];
    let mut mod_counts = vec![0usize; bin_count];
    let mut bin_load = vec![0usize; bin_count];
    for gi in by_weight {
        let lightest = (0..bin_count)
            .min_by_key(|&b| (bin_load[b], b))
            .unwrap_or(0);
        bins[lightest].extend_from_slice(&groups[gi]);
        mod_counts[lightest] += 1;
        bin_load[lightest] += groups[gi].len();
    }
    for bin in &mut bins {
        bin.sort_by_key(|id| rank_pos.get(id).copied().unwrap_or(usize::MAX));
    }
    (bins, mod_counts)
}

pub(super) fn rank_task_surface_nodes(
    task: &str,
    surface_cap: usize,
    ingest: &IngestResult,
    ranks: &[f64],
    interner: &Interner,
) -> Vec<NodeId> {
    let ranked = crate::query::retrieval::call_for_task_with_options(
        ingest,
        interner,
        ranks,
        task,
        crate::query::retrieval::ForTaskOptions {
            limit: Some(surface_cap),
            signatures_only: true,
            ..Default::default()
        },
    );
    let mut surface = Vec::new();
    let mut seen = BTreeSet::new();
    if let Some(arr) = ranked["symbols"].as_array() {
        for item in arr {
            let name = item["name"].as_str().unwrap_or("");
            let path = item["path"].as_str().unwrap_or("");
            if let Some(sym) = ingest.symbols.iter().find(|s| {
                interner.resolve(&s.name) == name && ingest.sym_file_path(s, interner) == path
            }) && seen.insert(sym.id)
            {
                surface.push(sym.id);
            }
        }
    }
    if surface.is_empty() {
        let mut all: Vec<NodeId> = ingest
            .symbols
            .iter()
            .filter(|s| s.kind != crate::domain::SymKind::Section)
            .map(|s| s.id)
            .collect();
        all.sort_by(|&a, &b| {
            let ra = ranks.get(a as usize).copied().unwrap_or(0.0);
            let rb = ranks.get(b as usize).copied().unwrap_or(0.0);
            rb.total_cmp(&ra).then_with(|| a.cmp(&b))
        });
        surface = all.into_iter().take(surface_cap).collect();
    }
    surface
}

#[must_use]
pub fn plan_task_partition(
    task: &str,
    requested: usize,
    ingest: &IngestResult,
    graph: &Graph,
    ranks: &[f64],
    interner: &Interner,
) -> PartitionPlan {
    let n = requested.clamp(1, 16);
    let surface_cap = 3 + 15 * n;
    let surface = rank_task_surface_nodes(task, surface_cap, ingest, ranks, interner);
    let rank_pos: BTreeMap<NodeId, usize> = surface
        .iter()
        .copied()
        .enumerate()
        .map(|(i, id)| (id, i))
        .collect();
    let core_count = if surface.len() > n {
        surface.len().min(3)
    } else {
        surface.len().min(1)
    };
    let core_ids = surface[..core_count].to_vec();
    let assignable = surface[core_count..].to_vec();
    if assignable.is_empty() {
        return PartitionPlan {
            core_ids,
            groups: Vec::new(),
            group_module_count: Vec::new(),
            surface_count: surface.len(),
            module_count: 0,
            split_count: 0,
        };
    }
    let cm = graph.communities();
    let mut groups = group_assignable_by_community(&assignable, ingest, graph, &cm);
    let module_count = groups.len();
    let split_count = split_groups_up_to(&mut groups, n);
    let bin_count = n.min(groups.len()).max(1);
    let (packed, group_module_count) = pack_groups_into_bins(&groups, bin_count, &rank_pos);
    PartitionPlan {
        core_ids,
        groups: packed,
        group_module_count,
        surface_count: surface.len(),
        module_count,
        split_count,
    }
}

fn pairwise_jaccard_stats(surfaces: &[BTreeSet<NodeId>]) -> (f64, f64) {
    let mut pair_count = 0usize;
    let mut sum = 0.0;
    let mut worst = 0.0_f64;
    for i in 0..surfaces.len() {
        for j in (i + 1)..surfaces.len() {
            let inter = surfaces[i].intersection(&surfaces[j]).count();
            let uni = surfaces[i].len() + surfaces[j].len() - inter;
            let jac = if uni == 0 {
                0.0
            } else {
                inter as f64 / uni as f64
            };
            sum += jac;
            pair_count += 1;
            worst = worst.max(jac);
        }
    }
    let mean = if pair_count == 0 {
        0.0
    } else {
        sum / pair_count as f64
    };
    (mean, worst)
}

#[must_use]
pub fn measure_partition_overlap(
    surfaces: &[BTreeSet<NodeId>],
    core_surface: &BTreeSet<NodeId>,
) -> OverlapStats {
    let (mean, worst) = pairwise_jaccard_stats(surfaces);
    let mut counts: BTreeMap<NodeId, usize> = BTreeMap::new();
    for s in surfaces {
        for &id in s {
            *counts.entry(id).or_insert(0) += 1;
        }
    }
    let union_count = counts.len();
    let shared_count = counts.values().filter(|&&c| c >= 2).count();
    let core_hits = core_surface
        .iter()
        .filter(|id| counts.contains_key(id))
        .count();
    let core_leak = if core_surface.is_empty() {
        0.0
    } else {
        core_hits as f64 / core_surface.len() as f64
    };
    OverlapStats {
        mean,
        worst,
        core_leak,
        shared_count,
        union_count,
    }
}
