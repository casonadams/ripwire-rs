//! Default repository map: printed when no other command is selected. Owns `--map-diff`, `--stable`,
//! `--no-stable`, `--order` and `--no-inventory`.

use super::{Command, SharedOption};
use crate::cli::Cli;
use crate::commands::context::{OutputDialect, load_repo_analysis_for_cli};
use crate::query::commands::CommandDescriptor;
use crate::query::map::{MapDiff, MapRequest, execute_stream_xml_ranked};
use clap::{Args, Command as ClapCommand};
use std::io;

mod tools;

/// Flags that shape the default repository map.
#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct MapArgs {
    /// Re-rank map toward git-changed files so recent work floats up
    #[arg(long = "map-diff")]
    pub map_diff: bool,
    /// Emit deterministic path-ordered map without volatile rank scores
    #[arg(long = "stable", overrides_with = "stable")]
    pub stable: bool,
    /// Disable the stable dependency violation section in --deps
    #[arg(long = "no-stable", overrides_with = "no_stable")]
    pub no_stable: bool,
    /// Emit ordering: important-first (default), important-last, or stable
    #[arg(long, default_value = "important-first")]
    pub order: String,
    /// Drop unranked <inv> file inventory tier from map and metrics output
    #[arg(long = "no-inventory", overrides_with = "no_inventory")]
    pub no_inventory: bool,
}

/// Decoded map flags; defaults when the command line was not parsed with this plugin.
#[must_use]
pub fn args(cli: &Cli) -> MapArgs {
    cli.plugins.decode::<MapArgs>().unwrap_or_default()
}

/// Print the default repository map for `root`.
pub fn dispatch_default_run(cli: &Cli, root: &str) -> io::Result<()> {
    let flags = args(cli);
    let emit_legend = !cli.is_legend_none();
    let legend_full = cli.legend.eq_ignore_ascii_case("full");
    let (ingest, graph, pr, interner) = load_repo_analysis_for_cli(cli, root, false)?;
    let mut stdout_lock = io::stdout().lock();
    let mut stdout = cli.wrap_xml_writer(&mut stdout_lock, false);
    let rank_by = if flags.stable || flags.order.eq_ignore_ascii_case("stable") {
        "stable"
    } else {
        &cli.rank_by
    };
    let (changed_files, at) = if flags.map_diff {
        let files = crate::git::repo::working_tree_changes(root).map_err(io::Error::other)?;
        let at = crate::git::history::compute_git_at_stamp(root).map_err(io::Error::other)?;
        (Some(files), at)
    } else {
        (None, None)
    };
    let mut request = MapRequest::new(root);
    request.rank_by = rank_by;
    request.pack_budget = cli.effective_budget_bytes();
    request.top_k = cli.top_k.or(request.top_k);
    request.emit_legend = emit_legend;
    request.legend_full = legend_full;
    request.map_diff = changed_files
        .as_ref()
        .zip(at.as_deref())
        .map(|(files, at)| MapDiff { files, at });
    request.no_inventory = flags.no_inventory;
    execute_stream_xml_ranked(&request, &ingest, &graph, &pr, &interner, &mut stdout)?;
    stdout.finish()
}

pub struct Map;

pub static MAP: Map = Map;

impl Command for Map {
    fn name(&self) -> &'static str {
        ""
    }

    fn augment(&self, cmd: ClapCommand) -> ClapCommand {
        MapArgs::augment_args(cmd)
    }

    fn run_default(&self, cli: &Cli, root: &str) -> Option<io::Result<()>> {
        Some(dispatch_default_run(cli, root))
    }

    fn options(&self) -> &'static [SharedOption] {
        &[SharedOption::Budget("Approximate token ceiling")]
    }

    fn tools(&self) -> &'static [&'static CommandDescriptor] {
        tools::TOOLS
    }
}
