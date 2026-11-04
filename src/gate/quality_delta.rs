mod acks;
mod cli;
mod clones;
mod compare;
mod model;
mod numeric;
mod render;
mod revision;
mod source_checks;

pub use self::acks::{
    StaleAck, ack_matching_regressions, content_id, parse_acks_content, split_ack_patterns,
};
pub use self::cli::{apply_cli_delta_options, apply_cli_delta_options_with_writer};
pub use self::clones::{MemberStatus, collect_changed_clone_regressions};
pub use self::compare::{compute_quality_delta, compute_quality_delta_with_acks};
pub use self::model::{
    DeltaBasis, DeltaCliOptions, QualityDeltaFacts, QualityDeltaReport, QualityRegression,
};
pub use self::numeric::{NumericSeverity, classify_numeric};
pub use self::render::stale_replay_for;
pub use self::revision::{
    WorkingTree, baseline_paths_to_read, load_revision_baseline_incremental, rebuild_baseline,
};
pub use self::source_checks::{body_text_kinds, is_changed_callable, symbol_matches_and_changed};

use self::acks::{apply_quality_acks, load_quality_acks};
use self::compare::compute_delta_and_renames;
use self::model::update_report_counts;
use self::revision::{attach_working_bodies, load_head_baseline_incremental};
use self::source_checks::{
    check_churn_regressions, check_source_and_clone_regressions, drop_referenced_dead_rows,
};
use crate::gate::baseline::{
    DEFAULT_BASELINE_FILE, QualitySnapshot, compute_snapshot, read_baseline_file,
};
use crate::query::commands::QueryContext;
use std::io;
use std::path::Path;

pub fn evaluate_quality_delta(
    root: &str,
    state: &QueryContext<'_>,
) -> io::Result<QualityDeltaReport> {
    evaluate_quality_delta_with_basis(root, state, DeltaBasis::PinnedOrHead)
}

#[must_use]
pub fn resolve_delta_head_sha(at_stamp: &str) -> &str {
    at_stamp
        .strip_suffix("+dirty")
        .unwrap_or(if at_stamp.is_empty() {
            "HEAD"
        } else {
            at_stamp
        })
}

pub fn assemble_quality_delta_report(
    state: &QueryContext<'_>,
    facts: &QualityDeltaFacts<'_>,
) -> QualityDeltaReport {
    let (mut report, renames) =
        compute_delta_and_renames(facts.working, facts.baseline, facts.source_name, facts.acks);
    drop_referenced_dead_rows(state, &mut report.regressions);
    report.head_basis = facts.head_basis;
    report.baseline_absorbed = facts.baseline.absorbed;
    if !facts.at_stamp.is_empty() {
        report.at_stamp = facts.at_stamp.to_string();
    }
    if !facts.changed_files.is_empty() {
        check_churn_regressions(
            state,
            facts.baseline,
            facts.changed_files,
            &mut report.regressions,
        );
        check_source_and_clone_regressions(state, facts, &renames, &mut report.regressions);
        if let Some(ledger) = facts.acks {
            report.acked_count += apply_quality_acks(&mut report.regressions, ledger);
        }
    }
    update_report_counts(&mut report);
    report
}

fn try_read_pinned_baseline(root: &str, basis: DeltaBasis) -> io::Result<Option<QualitySnapshot>> {
    if basis == DeltaBasis::PinnedOrHead {
        let baseline_path = Path::new(root).join(DEFAULT_BASELINE_FILE);
        if baseline_path.exists() {
            return read_baseline_file(&baseline_path).map(Some);
        }
    }
    Ok(None)
}

fn resolve_quality_baseline(
    root: &str,
    state: &QueryContext<'_>,
    basis: DeltaBasis,
    working: &QualitySnapshot,
    changed_files: &[String],
) -> io::Result<(QualitySnapshot, &'static str, Option<&'static str>)> {
    if let Some(snap) = try_read_pinned_baseline(root, basis)? {
        return Ok((snap, "sidecar", None));
    }
    if changed_files.is_empty() {
        Ok((working.clone(), "git-HEAD", Some("identity")))
    } else {
        let snap =
            load_head_baseline_incremental(root, state, working, changed_files, &working.head_sha);
        Ok((snap, "git-HEAD", None))
    }
}

/// Clone groups with a member in a changed file; nothing when the tree has not changed.
fn collect_delta_clones(
    state: &QueryContext<'_>,
    root: &str,
    changed_files: &[String],
) -> Vec<crate::clones::CloneGroup> {
    if changed_files.is_empty() {
        return Vec::new();
    }
    let focus: std::collections::HashSet<u32> = state
        .ingest
        .files
        .iter()
        .filter(|f| {
            let path = state.interner.resolve(&f.path);
            changed_files
                .iter()
                .any(|cf| path == cf || path.ends_with(cf.as_str()))
        })
        .map(|f| f.id)
        .collect();
    crate::clones::find_clones_touching(
        state.ingest,
        state.interner,
        crate::clones::DEFAULT_MIN_CLONE_TOKENS,
        crate::clones::CloneSource::Root(Path::new(root)),
        &focus,
    )
}

pub fn evaluate_quality_delta_with_basis(
    root: &str,
    state: &QueryContext<'_>,
    basis: DeltaBasis,
) -> io::Result<QualityDeltaReport> {
    let at_stamp = crate::git::history::compute_git_at_stamp(root)
        .map_err(io::Error::other)?
        .unwrap_or_default();
    let head_sha = resolve_delta_head_sha(&at_stamp);
    let changed_files = crate::git::repo::working_tree_changes(root).map_err(io::Error::other)?;
    let mut working = compute_snapshot(state.ingest, state.graph, state.interner, head_sha);
    working.dirty = !changed_files.is_empty();
    attach_working_bodies(&mut working, state, &changed_files);

    let (baseline, source_name, head_basis) =
        resolve_quality_baseline(root, state, basis, &working, &changed_files)?;
    let acks = load_quality_acks(root);
    let clones = collect_delta_clones(state, root, &changed_files);

    let facts = QualityDeltaFacts {
        working: &working,
        baseline: &baseline,
        source_name,
        head_basis,
        at_stamp: &at_stamp,
        changed_files: &changed_files,
        acks: acks.as_ref(),
        clones: &clones,
    };
    Ok(assemble_quality_delta_report(state, &facts))
}

/// The delta against the merge base of `base` when given, else against the pinned sidecar or HEAD.
pub fn evaluate_quality_delta_from(
    state: &QueryContext<'_>,
    base: Option<&str>,
) -> io::Result<QualityDeltaReport> {
    match base {
        Some(spec) => evaluate_quality_delta_since_ref(state.root, state, spec),
        None => evaluate_quality_delta(state.root, state),
    }
}

/// Quality delta of the working tree against the merge base of `spec` (`REV` or `REV..HEAD`) and
/// HEAD, rebuilt from git: what everything since that base made worse. No sidecar is read.
fn evaluate_quality_delta_since_ref(
    root: &str,
    state: &QueryContext<'_>,
    spec: &str,
) -> io::Result<QualityDeltaReport> {
    let fail = |e: String| io::Error::other(format!("--quality-delta: {e}"));
    let changes = crate::git::repo::changes_since_spec(root, spec).map_err(fail)?;
    let at_stamp = crate::git::history::compute_git_at_stamp(root)
        .map_err(io::Error::other)?
        .unwrap_or_default();
    let head_sha = resolve_delta_head_sha(&at_stamp);
    let changed_files = changes.paths;
    let mut working = compute_snapshot(state.ingest, state.graph, state.interner, head_sha);
    working.dirty = !changed_files.is_empty();
    attach_working_bodies(&mut working, state, &changed_files);

    let (baseline, head_basis) = if changed_files.is_empty() {
        (working.clone(), Some("identity"))
    } else {
        let snap = load_revision_baseline_incremental(
            root,
            &changes.base,
            state.ingest,
            state.interner,
            &working,
            &changed_files,
            head_sha,
        );
        (snap, None)
    };
    let acks = load_quality_acks(root);
    let clones = collect_delta_clones(state, root, &changed_files);

    let facts = QualityDeltaFacts {
        working: &working,
        baseline: &baseline,
        source_name: "git-ref",
        head_basis,
        at_stamp: &at_stamp,
        changed_files: &changed_files,
        acks: acks.as_ref(),
        clones: &clones,
    };
    let mut report = assemble_quality_delta_report(state, &facts);
    report.base_ref = Some(changes.base.get(..9).unwrap_or(&changes.base).to_string());
    Ok(report)
}
