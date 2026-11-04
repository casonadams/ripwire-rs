use super::acks::{
    DEFAULT_ACKS_FILE, ack_matching_regressions, regression_content_id, split_ack_patterns,
};
use super::model::{DeltaCliOptions, QualityDeltaReport, QualityRegression};
use super::render::stale_replay_for;
use crate::query::commands::QueryContext;
use std::io;
use std::path::Path;

fn filter_regressions_by_scope(
    root: &str,
    regressions: &mut [QualityRegression],
    scope_spec: &str,
) -> io::Result<()> {
    let changed = if scope_spec == "diff" {
        crate::git::repo::working_tree_changes(root).map_err(io::Error::other)?
    } else {
        Vec::new()
    };
    let pats: Vec<&str> = scope_spec.split(',').map(str::trim).collect();
    for r in regressions {
        let in_scope = if scope_spec == "diff" {
            changed
                .iter()
                .any(|cf| r.file_path == *cf || r.file_path.ends_with(cf))
        } else {
            pats.iter().any(|&p| r.file_path.contains(p))
        };
        if !in_scope {
            r.is_gating = false;
        }
    }
    Ok(())
}

pub fn apply_cli_delta_options_with_writer<W: std::io::Write>(
    state: &QueryContext<'_>,
    report: &mut QualityDeltaReport,
    opts: DeltaCliOptions<'_>,
    ack_writer: impl FnOnce() -> io::Result<W>,
) -> io::Result<()> {
    let root = state.root;
    let scope_opt = opts.scope.filter(|s| !s.is_empty());
    if let Some(scope_spec) = scope_opt {
        filter_regressions_by_scope(root, &mut report.regressions, scope_spec)?;
    }
    if !opts.show_stale {
        report.stale_collapse_replay = Some(stale_replay_for(scope_opt));
    }
    if let Some(reason) = opts.quality_ack {
        let ack_pats = opts.ack_only.map(split_ack_patterns).unwrap_or_default();
        let mut file = ack_writer()?;
        let cid_for = |r: &QualityRegression| regression_content_id(state, r);
        ack_matching_regressions(report, &ack_pats, reason, cid_for, &mut file)?;
    }
    report.gating_count = report.regressions.iter().filter(|r| r.is_gating).count();
    report.minor_count = report.regressions.len().saturating_sub(report.gating_count);
    Ok(())
}

pub fn apply_cli_delta_options(
    state: &QueryContext<'_>,
    report: &mut QualityDeltaReport,
    opts: DeltaCliOptions<'_>,
) -> io::Result<()> {
    apply_cli_delta_options_with_writer(state, report, opts, || {
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(Path::new(state.root).join(DEFAULT_ACKS_FILE))
    })
}
