use super::env::{LaneBuildEnv, ambiguity_of};
use crate::domain::NodeId;
use serde_json::{Value, json};
use std::collections::BTreeMap;

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

pub(super) struct LaneClaimsSummary {
    pub(super) sym_rows: Vec<Value>,
    pub(super) file_rows: Vec<Value>,
    pub(super) claim_keys: BTreeMap<u64, (String, String, NodeId)>,
    pub(super) file_counts: BTreeMap<String, usize>,
    pub(super) max_ccx: u32,
    pub(super) sum_ccx: u32,
    pub(super) untested: usize,
}

pub(super) fn build_lane_claims_and_files(
    node_ids: &[NodeId],
    env: &LaneBuildEnv<'_>,
) -> LaneClaimsSummary {
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
