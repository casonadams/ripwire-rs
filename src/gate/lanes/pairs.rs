use super::lane::BuiltLane;
use serde_json::{Value, json};

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

pub(super) fn compute_pairs_and_landing_order(
    lanes: &mut [BuiltLane],
) -> (Vec<Value>, Vec<String>) {
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
