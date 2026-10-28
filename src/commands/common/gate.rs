//! Shared gate plumbing: workspace loading for gate verbs, and modifier validation.

use crate::cli::Cli;
use crate::commands::common::changes::get_working_tree_changed_files;
use crate::commands::context::{OutputDialect, load_repo_analysis_for_cli};
use crate::commands::quality_baseline::QualityBaselineFlags;
use crate::commands::quality_delta::QualityDeltaFlags;
use crate::serialize::XmlWriter;
use std::io::{self, StdoutLock};

/// Loads the analysis for a gate verb; delta gates also load the scoped history.
pub fn load_gate_state(cli: &Cli, root: &str, is_delta: bool) -> io::Result<crate::Workspace> {
    let (ingest, graph, pr, interner) = load_repo_analysis_for_cli(cli, root, is_delta)?;
    let mut state = crate::Workspace::from_analysis(root, ingest, graph, pr, interner);
    state.at = crate::git::history::compute_git_at_stamp(root);
    if is_delta && !get_working_tree_changed_files(root).is_empty() {
        let cutoff = crate::git::history::resolve_cutoff_timestamp(None, 1);
        state.commits = crate::git::history::walk_repository_history_scoped(root, cutoff).ok();
    }
    Ok(state)
}

/// Loads the gate workspace, runs `body` on the shared XML writer, and finishes the writer on success.
pub fn with_workspace<T>(
    cli: &Cli,
    root: &str,
    is_delta: bool,
    body: impl FnOnce(&crate::Workspace, &mut XmlWriter<'_, StdoutLock<'static>>) -> io::Result<T>,
) -> io::Result<T> {
    let state = load_gate_state(cli, root, is_delta)?;
    let mut stdout_lock = io::stdout().lock();
    let mut out = OutputDialect::wrap_xml_writer(cli, &mut stdout_lock, false);
    let value = body(&state, &mut out)?;
    out.finish()?;
    Ok(value)
}

/// Refuses gate modifiers passed without the verb they shape.
pub fn validate_gate_modifiers(
    allow_dirty: bool,
    quality_baseline: bool,
    show_stale: bool,
    quality_delta: bool,
) -> io::Result<()> {
    if allow_dirty && !quality_baseline {
        return Err(io::Error::other(
            "ripwire-rs: --allow-dirty modifies --quality-baseline; pass it too",
        ));
    }
    if show_stale && !quality_delta {
        return Err(io::Error::other(
            "ripwire-rs: --show-stale modifies --quality-delta; pass it too",
        ));
    }
    Ok(())
}

/// Validates the gate modifiers parsed from the command line (quality-ack implies quality-delta).
pub fn validate_cli_gate_modifiers(cli: &Cli) -> io::Result<()> {
    let baseline = cli
        .plugins
        .decode::<QualityBaselineFlags>()
        .unwrap_or_default();
    let delta = cli
        .plugins
        .decode::<QualityDeltaFlags>()
        .unwrap_or_default();
    validate_gate_modifiers(
        baseline.opts.allow_dirty,
        baseline.primary.quality_baseline,
        delta.opts.show_stale,
        delta.primary.requested() || delta.opts.quality_ack.is_some(),
    )
}
