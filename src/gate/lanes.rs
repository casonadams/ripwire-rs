use crate::domain::{IngestResult, Interner, NodeId, SymKind};
use crate::graph::{Graph, community::Communities};
use crate::rank::PageRankResult;
use crate::serialize::{PageRequest, Paged, text_bytes};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet, HashMap};

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

fn fnv1a64_claim_key(path: &str, scope: &str, name: &str) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for slice in [
        path.as_bytes(),
        &[0],
        scope.as_bytes(),
        &[0],
        name.as_bytes(),
    ] {
        for &b in slice {
            hash ^= u64::from(b);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    hash
}

fn group_key_for_node(id: NodeId, ingest: &IngestResult, graph: &Graph, cm: &Communities) -> u64 {
    if graph.out_degree(id) > 0 || graph.in_degree(id) > 0 {
        u64::from(cm.comm.get(id as usize).copied().unwrap_or(0))
    } else {
        let file_id = ingest.sym(id).map_or(0, |s| s.file_id);
        (1u64 << 32) | u64::from(file_id)
    }
}

fn group_assignable_by_community(
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

fn rank_task_surface_nodes(
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

struct BuiltLane {
    id: String,
    lane_num: usize,
    node_ids: Vec<NodeId>,
    claim_keys: BTreeMap<u64, (String, String, NodeId)>,
    file_counts: BTreeMap<String, usize>,
    blast_ids: BTreeSet<NodeId>,
    json_val: Value,
}

struct LaneClaimsSummary {
    sym_rows: Vec<Value>,
    file_rows: Vec<Value>,
    claim_keys: BTreeMap<u64, (String, String, NodeId)>,
    file_counts: BTreeMap<String, usize>,
    max_ccx: u32,
    sum_ccx: u32,
    untested: usize,
}

struct LaneBuildEnv<'a> {
    ingest: &'a IngestResult,
    graph: &'a Graph,
    ranks: &'a [f64],
    interner: &'a Interner,
    churn: Option<&'a HashMap<String, u32>>,
    index: &'a LaneIndex,
    page: PageRequest,
}

/// Corpus-wide tables every lane reads: test reach, file hotspot ranks, and definitions per `(scope, name)`.
struct LaneIndex {
    tested_mask: Vec<bool>,
    hot_ranks: HashMap<String, usize>,
    id_defs: HashMap<(String, String), usize>,
}

fn lane_index(corpus: LaneCorpus<'_>) -> LaneIndex {
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
    let hot_ranks = corpus.churn.map_or_else(HashMap::new, |churn| {
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

fn lane_env<'a>(
    corpus: LaneCorpus<'a>,
    index: &'a LaneIndex,
    page: PageRequest,
) -> LaneBuildEnv<'a> {
    LaneBuildEnv {
        ingest: corpus.ingest,
        graph: corpus.graph,
        ranks: &corpus.pr.ranks,
        interner: corpus.interner,
        churn: corpus.churn,
        index,
        page,
    }
}

fn ambiguity_of(graph: &Graph, id: NodeId) -> usize {
    graph.amb_out.get(id as usize).map_or(0, |&n| n as usize)
}

/// One symbol a lane claims, with the identity its claim row and claim key derive from.
struct ClaimedSymbol {
    id: NodeId,
    ord: usize,
    key: u64,
    path: String,
    name: String,
    scope: String,
    is_tested: bool,
}

fn claimed_symbol(
    id: NodeId,
    ord: usize,
    sym: &crate::domain::Symbol,
    env: &LaneBuildEnv<'_>,
) -> ClaimedSymbol {
    let interner = env.interner;
    let path = env.ingest.sym_file_path(sym, interner).to_string();
    let name = interner.resolve(&sym.name).to_string();
    let scope = sym.scope.map_or("", |sc| interner.resolve(&sc)).to_string();
    let key = fnv1a64_claim_key(&path, &scope, &name);
    let is_tested = env
        .index
        .tested_mask
        .get(id as usize)
        .copied()
        .unwrap_or(false)
        || crate::domain::test_path::is_test_path(&path);
    ClaimedSymbol {
        id,
        ord,
        key,
        path,
        name,
        scope,
        is_tested,
    }
}

fn claim_row(claim: &ClaimedSymbol, sym: &crate::domain::Symbol, env: &LaneBuildEnv<'_>) -> Value {
    let defs = env
        .index
        .id_defs
        .get(&(claim.scope.clone(), claim.name.clone()))
        .copied()
        .unwrap_or(1);
    let churn = env.churn.map(|c| c.get(&claim.path).copied().unwrap_or(0));
    let addressable = !claim.scope.is_empty();
    let mut row = json!({
        "p": claim.path,
        "n": claim.name,
        "scope": claim.scope,
        "key": format!("{:016x}", claim.key),
        "id": if addressable { json!(format!("{}::{}", claim.scope, claim.name)) } else { Value::Null },
        "id_addressable": addressable,
        "l": sym.line,
        "ord": claim.ord,
        "overloads": defs,
        "amb": ambiguity_of(env.graph, claim.id),
        "cx": sym.cx.max(1),
        "ccx": sym.ccx,
        "tested": usize::from(claim.is_tested),
    });
    if addressable {
        row["id_collides_with"] = json!(defs - 1);
    }
    if let Some(churn) = churn {
        row["churn"] = json!(churn);
    }
    row
}

fn file_claim_rows(
    file_stats: BTreeMap<String, (usize, u32)>,
    env: &LaneBuildEnv<'_>,
) -> Vec<Value> {
    file_stats
        .into_iter()
        .map(|(p, (cnt, ccx))| {
            let churn = env.churn.map(|c| c.get(&p).copied().unwrap_or(0));
            let rank = env.index.hot_ranks.get(&p).copied();
            let mut row = json!({ "p": p, "symbols": cnt, "ccx": ccx });
            if let Some(churn) = churn {
                row["churn"] = json!(churn);
                row["hotspot_rank"] = json!(rank);
            }
            row
        })
        .collect()
}

fn build_lane_claims_and_files(node_ids: &[NodeId], env: &LaneBuildEnv<'_>) -> LaneClaimsSummary {
    let mut sym_rows = Vec::new();
    let mut claim_keys = BTreeMap::new();
    let mut file_stats: BTreeMap<String, (usize, u32)> = BTreeMap::new();
    let mut max_ccx = 0u32;
    let mut sum_ccx = 0u32;
    let mut untested = 0usize;
    for (ord, &id) in node_ids.iter().enumerate() {
        let Some(sym) = env.ingest.sym(id) else {
            continue;
        };
        let claim = claimed_symbol(id, ord, sym, env);
        if !claim.is_tested {
            untested += 1;
        }
        max_ccx = max_ccx.max(sym.ccx);
        sum_ccx = sum_ccx.saturating_add(sym.ccx);
        claim_keys.insert(claim.key, (claim.path.clone(), claim.name.clone(), id));
        let entry = file_stats.entry(claim.path.clone()).or_insert((0, 0));
        entry.0 += 1;
        entry.1 = entry.1.saturating_add(sym.ccx);
        sym_rows.push(claim_row(&claim, sym, env));
    }
    let file_counts: BTreeMap<String, usize> = file_stats
        .iter()
        .map(|(k, (cnt, _))| (k.clone(), *cnt))
        .collect();
    LaneClaimsSummary {
        sym_rows,
        file_rows: file_claim_rows(file_stats, env),
        claim_keys,
        file_counts,
        max_ccx,
        sum_ccx,
        untested,
    }
}

/// What a lane reaches through the call graph: its blast radius, and the files it touches there.
struct LaneBlast {
    reaches: usize,
    files_total: usize,
    capped: bool,
    files: Vec<String>,
    ids: BTreeSet<NodeId>,
}

fn lane_blast(node_ids: &[NodeId], env: &LaneBuildEnv<'_>) -> LaneBlast {
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
struct LaneTests {
    total: usize,
    capped: bool,
    to_run: Vec<String>,
}

fn lane_tests(file_counts: &BTreeMap<String, usize>, env: &LaneBuildEnv<'_>) -> LaneTests {
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

fn lane_model(max_ccx: u32, reaches: usize) -> &'static str {
    if max_ccx >= 15 || reaches > 20 {
        "frontier"
    } else {
        "standard"
    }
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

fn build_single_lane(
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

fn collect_contract_touches(from: &BuiltLane, to: &BuiltLane, out: &mut Vec<Value>) {
    for (&k, (path, name, id)) in &from.claim_keys {
        if to.blast_ids.contains(id) {
            out.push(json!({
                "p": path,
                "n": name,
                "key": format!("{k:016x}"),
                "from": from.id,
                "to": to.id,
            }));
        }
    }
}

fn compute_lane_pair_row(a: &BuiltLane, b: &BuiltLane) -> (Value, usize, usize) {
    let conflicts: Vec<Value> = a
        .claim_keys
        .iter()
        .filter(|(k, _)| b.claim_keys.contains_key(k))
        .map(|(&k, (path, name, _))| {
            json!({
                "key": format!("{k:016x}"),
                "p": path,
                "n": name,
            })
        })
        .collect();
    let mut same_file_risk = Vec::new();
    let mut risk_pair_count = 0usize;
    for (path, &a_cnt) in &a.file_counts {
        if let Some(&b_cnt) = b.file_counts.get(path) {
            let pairs = a_cnt * b_cnt;
            if pairs > conflicts.len() {
                risk_pair_count += pairs;
                same_file_risk.push(json!({
                    "p": path,
                    "a_symbols": a_cnt,
                    "b_symbols": b_cnt,
                    "pairs": pairs,
                }));
            }
        }
    }
    let mut contract_touch = Vec::new();
    collect_contract_touches(a, b, &mut contract_touch);
    collect_contract_touches(b, a, &mut contract_touch);
    let conflict_count = conflicts.len();
    let touch_count = contract_touch.len();
    let row = json!({
        "a": a.id,
        "b": b.id,
        "conflicts": conflicts,
        "conflict_count": conflict_count,
        "same_file_risk": same_file_risk,
        "risk_count": risk_pair_count,
        "contract_touch": contract_touch,
        "touch_count": touch_count,
    });
    (row, conflict_count, touch_count)
}

fn compute_pairs_and_landing_order(lanes: &mut [BuiltLane]) -> (Vec<Value>, Vec<String>) {
    let mut pairs = Vec::new();
    let mut conflict_deg = vec![0usize; lanes.len()];
    let mut touch_deg = vec![0usize; lanes.len()];
    for i in 0..lanes.len() {
        for j in (i + 1)..lanes.len() {
            let (row, c_cnt, t_cnt) = compute_lane_pair_row(&lanes[i], &lanes[j]);
            conflict_deg[i] += c_cnt;
            conflict_deg[j] += c_cnt;
            touch_deg[i] += t_cnt;
            touch_deg[j] += t_cnt;
            pairs.push(row);
        }
    }
    for (i, lane) in lanes.iter_mut().enumerate() {
        let signals = &mut lane.json_val["execution"]["signals"];
        signals["conflicts"] = json!(conflict_deg[i]);
        signals["contract_touches"] = json!(touch_deg[i]);
    }
    let mut order_indices: Vec<usize> = (0..lanes.len()).collect();
    order_indices.sort_by_key(|&i| {
        (
            conflict_deg[i],
            touch_deg[i],
            lanes[i].blast_ids.len(),
            lanes[i].lane_num,
        )
    });
    let landing_order = order_indices
        .into_iter()
        .map(|i| lanes[i].id.clone())
        .collect();
    (pairs, landing_order)
}

fn build_core_json(core_ids: &[NodeId], ingest: &IngestResult, interner: &Interner) -> Value {
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

fn assemble_plan_lanes_envelope(
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

fn attach_corpus_stats(mut val: Value, ingest: &IngestResult, graph: &Graph) -> Value {
    let gauges = graph.gauges();
    val["corpus"] = json!({
        "files": ingest.files.len(),
        "symbols": ingest.symbol_count(),
        "edges": graph.edge_count(),
        "ambiguous": gauges.ambiguous,
        "unresolved": gauges.unresolved,
    });
    val
}

/// The indexed corpus a plan-lanes run reads: the root for the envelope, the index, PageRank ranks, and the
/// per-file commit counts over the git history window (`None` when the root is not a git repository).
#[derive(Clone, Copy)]
pub struct LaneCorpus<'a> {
    pub root: &'a str,
    pub ingest: &'a IngestResult,
    pub graph: &'a Graph,
    pub pr: &'a PageRankResult,
    pub interner: &'a Interner,
    pub churn: Option<&'a HashMap<String, u32>>,
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
    attach_corpus_stats(base, corpus.ingest, corpus.graph)
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
    attach_corpus_stats(base, corpus.ingest, corpus.graph)
}
