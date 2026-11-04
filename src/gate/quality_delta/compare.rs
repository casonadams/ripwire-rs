use super::acks::{QualityAcks, apply_quality_acks, collect_stale_acks};
use super::model::{QualityDeltaReport, QualityRegression, update_report_counts};
use super::numeric::check_symbol_numeric_regressions;
use crate::gate::baseline::QualitySnapshot;
use crate::gate::rename::{RenameMap, pair_renames};

fn check_structural_regressions(
    working: &QualitySnapshot,
    baseline: &QualitySnapshot,
    renames: &RenameMap,
    regressions: &mut Vec<QualityRegression>,
) {
    let mut dead_syms: Vec<&String> = working.dead_symbols.iter().collect();
    dead_syms.sort_unstable();
    for dead in dead_syms {
        let was_key = renames.was_key(dead);
        if !baseline.dead_symbols.contains(was_key) {
            let is_new = !baseline.ccx_by_sym.contains_key(was_key);
            regressions.push(QualityRegression {
                kind: "dead-code".to_string(),
                sym_name: dead.clone(),
                file_path: dead.split("::").next().unwrap_or(dead).to_string(),
                line: working.line_by_sym.get(dead).copied().unwrap_or(0),
                was_val: 0,
                now_val: 1,
                is_gating: !is_new,
                is_new_symbol: is_new,
                renamed_from: renames.pair_for(dead).map(|p| p.old_name.clone()),
                ..QualityRegression::default()
            });
        }
    }
    let mut public_syms: Vec<&String> = working.public_api.iter().collect();
    public_syms.sort_unstable();
    for pub_sym in public_syms {
        if baseline.public_api.contains(renames.was_key(pub_sym)) {
            continue;
        }
        regressions.push(QualityRegression {
            kind: "api-surface".to_string(),
            sym_name: pub_sym.split("::").last().unwrap_or(pub_sym).to_string(),
            file_path: pub_sym.split("::").next().unwrap_or(pub_sym).to_string(),
            line: working.line_by_sym.get(pub_sym).copied().unwrap_or(0),
            was_val: 0,
            now_val: 1,
            is_gating: false,
            is_new_symbol: true,
            renamed_from: None,
            ..QualityRegression::default()
        });
    }
}

/// Compute quality delta report comparing working tree snapshot against baseline.
#[must_use]
pub fn compute_quality_delta(
    working: &QualitySnapshot,
    baseline: &QualitySnapshot,
    baseline_source: &str,
) -> QualityDeltaReport {
    compute_quality_delta_with_acks(working, baseline, baseline_source, None)
}

/// Compute quality delta report comparing working tree snapshot against baseline with acks.
#[must_use]
pub fn compute_quality_delta_with_acks(
    working: &QualitySnapshot,
    baseline: &QualitySnapshot,
    baseline_source: &str,
    acks: Option<&QualityAcks>,
) -> QualityDeltaReport {
    compute_delta_and_renames(working, baseline, baseline_source, acks).0
}

/// The delta report with the rename pairs it was computed under, so rows added later see the same pairing.
pub(super) fn compute_delta_and_renames(
    working: &QualitySnapshot,
    baseline: &QualitySnapshot,
    baseline_source: &str,
    acks: Option<&QualityAcks>,
) -> (QualityDeltaReport, RenameMap) {
    let renames = pair_renames(working, baseline);
    let mut regressions = Vec::new();

    let mut keys: Vec<&String> = working.ccx_by_sym.keys().collect();
    keys.sort_unstable();
    for key in keys {
        check_symbol_numeric_regressions(key, working, baseline, &renames, &mut regressions);
    }

    check_structural_regressions(working, baseline, &renames, &mut regressions);

    let (acked_count, stale_acks, acked_by_rename) = match acks {
        Some(ledger) => {
            let stale = collect_stale_acks(ledger, &regressions, &renames);
            let carried = ledger
                .entries
                .iter()
                .filter(|e| renames.carries(&e.key))
                .count();
            (apply_quality_acks(&mut regressions, ledger), stale, carried)
        }
        None => (0, Vec::new(), 0),
    };
    let api_new_surface = working
        .public_api
        .iter()
        .filter(|key| !baseline.public_api.contains(renames.was_key(key)))
        .count();
    let short_sha = if working.head_sha.len() >= 9 {
        &working.head_sha[..9]
    } else {
        &working.head_sha
    };
    let at_stamp = if working.dirty {
        format!("{short_sha}+dirty")
    } else {
        short_sha.to_string()
    };
    let mut report = QualityDeltaReport {
        baseline_source: baseline_source.to_string(),
        head_sha: working.head_sha.clone(),
        at_stamp,
        head_basis: None,
        regressions,
        stale_acks,
        acked_count,
        gating_count: 0,
        preexisting_worse_count: 0,
        new_symbol_count: 0,
        minor_count: 0,
        api_new_surface,
        stale_collapse_replay: None,
        baseline_absorbed: 0,
        renames: renames.len(),
        acked_by_rename,
        base_ref: None,
    };
    update_report_counts(&mut report);
    (report, renames)
}
