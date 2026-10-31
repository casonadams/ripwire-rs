//! Command plugins: each module under `src/commands/<name>/` owns the CLI flags, CLI handler and MCP tools
//! of one command group. Shared code only sees the [`Command`] trait; adding a command is a new module
//! plus one line in `register_commands!`.
use crate::cli::Cli;
use crate::query::commands::CommandDescriptor;

pub mod plugin;
pub use plugin::{Plugin, Run, SharedOption};

pub trait Command: Sync {
    /// The command's primary flag or subcommand, without `--`; empty for plugins that are not a single command.
    fn name(&self) -> &'static str;

    /// Shared options (`--limit`, ...) this command honors; empty when it honors none.
    fn options(&self) -> &'static [SharedOption];

    /// Add this command's clap flags (a derive `Args::augment_args`) to the CLI.
    fn augment(&self, cmd: clap::Command) -> clap::Command {
        cmd
    }

    /// Subcommands this command contributes to the CLI.
    fn subcommands(&self) -> Vec<clap::Command> {
        Vec::new()
    }

    /// Run the command when its flags are present on the command line; `None` when they are not.
    fn run(&self, _cli: &Cli, _root: &str) -> Option<std::io::Result<()>> {
        None
    }

    /// Run subcommand `name` (its own parsed `matches`) when it was selected; `None` otherwise.
    fn run_subcommand(
        &self,
        _cli: &Cli,
        _name: &str,
        _matches: &clap::ArgMatches,
        _root: &str,
    ) -> Option<std::io::Result<()>> {
        None
    }

    /// MCP tool descriptors this command contributes.
    fn tools(&self) -> &'static [&'static CommandDescriptor] {
        &[]
    }

    /// Reject invalid flag combinations before any command runs.
    fn validate(&self, _cli: &Cli) -> std::io::Result<()> {
        Ok(())
    }

    /// Whether this command asks for a fresh clone of a remote checkout (`--refetch`).
    fn refreshes_remote_checkout(&self, _cli: &Cli) -> bool {
        false
    }

    /// The default output, when no plugin claimed the command line; `None` when not the default.
    fn run_default(&self, _cli: &Cli, _root: &str) -> Option<std::io::Result<()>> {
        None
    }
}

pub mod common;
pub mod context;
/// One line per command plugin; order is CLI dispatch precedence.
macro_rules! register_commands {
    ($($module:ident :: $instance:ident,)*) => {
        $(pub mod $module;)*

        static REGISTRY: &[&dyn Command] = &[$(&$module::$instance),*];
    };
}

register_commands! {
    mcp::MCP,
    skills::SKILLS,
    eval::EVAL,
    batch::BATCH,
    nav::NAV,
    callers::CALLERS,
    callees::CALLEES,
    impact::IMPACT,
    uses::USES,
    find_symbol::FIND_SYMBOL,
    pack_task_partitioned::PACK_TASK_PARTITIONED,
    recall::RECALL,
    graph_query::GRAPH_QUERY,
    for_task::FOR_TASK,
    grep::GREP,
    regex::REGEX,
    pack_signatures::PACK_SIGNATURES,
    connect::CONNECT,
    defs::DEFS,
    path_between::PATH_BETWEEN,
    exemplar::EXEMPLAR,
    lego::LEGO,
    from_trace::FROM_TRACE,
    at::AT,
    verify::VERIFY,
    pack_task::PACK_TASK,
    pattern::PATTERN,
    ast_match::AST_MATCH,
    help_task::HELP_TASK,
    refetch::REFETCH,
    orient::ORIENT,
    owners::OWNERS,
    stray_content::STRAY_CONTENT,
    replace_symbol_body::REPLACE_SYMBOL_BODY,
    insert_before_symbol::INSERT_BEFORE_SYMBOL,
    insert_after_symbol::INSERT_AFTER_SYMBOL,
    whereis::WHEREIS,
    doc_drift::DOC_DRIFT,
    flags_report::FLAGS,
    mentions::MENTIONS,
    doctor::DOCTOR,
    handoff::HANDOFF,
    notes::NOTES,
    note_add::NOTE_ADD,
    flip::FLIP,
    plan_lint::PLAN_LINT,
    skipped::SKIPPED,
    expand::EXPAND,
    outline::OUTLINE,
    pack_top_n::PACK_TOP_N,
    tree::TREE,
    biggest_first::BIGGEST_FIRST,
    nonlocal_state::NONLOCAL_STATE,
    lint::LINT,
    lint_catalog::LINT_CATALOG,
    naming_locals::NAMING_LOCALS,
    naming_consistency::NAMING_CONSISTENCY,
    naming_calibration::NAMING_CALIBRATION,
    report::REPORT,
    legend_dict::LEGEND_DICT,
    test_gate::TEST_GATE,
    situ::SITU,
    safe_delete::SAFE_DELETE,
    exercises::EXERCISES,
    seams::SEAMS,
    pr_context::PR_CONTEXT,
    merge_scout::MERGE_SCOUT,
    dmm::DMM,
    plan_lanes::PLAN_LANES,
    scan_skills::SCAN_SKILLS,
    edit_plan::EDIT_PLAN,
    partition::PARTITION,
    pin_census::PIN_CENSUS,
    baseline::BASELINE,
    map::MAP,
    mermaid::MERMAID,
    arch::ARCH,
    zoom::ZOOM,
    dead_code::DEAD_CODE,
    communities::COMMUNITIES,
    around::AROUND,
    external_surface::EXTERNAL_SURFACE,
    deps::DEPS,
    symbol_metrics::SYMBOL_METRICS,
    quality_panel::QUALITY_PANEL,
    layout::LAYOUT,
    field_affinity::FIELD_AFFINITY,
    context_ratio::CONTEXT_RATIO,
    ensemble::ENSEMBLE,
    comment_coherence::COMMENT_COHERENCE,
    cochange::COCHANGE,
    hotspots::HOTSPOTS,
    metrics::METRICS,
    edit_check::EDIT_CHECK,
    affected::AFFECTED,
    quality_baseline::QUALITY_BASELINE,
    quality_delta::QUALITY_DELTA,
    gate::GATE,
    clones::CLONES,
    html::HTML,
    export::EXPORT,
    index_out::INDEX_OUT,
    slice::SLICE,
}

/// The flags `-h` lists: the Find and Check verbs and the options that shape their answers. Every other
/// flag (reports, lenses, edits, exports) stays in `--help`, so an agent's first look is a short list.
const CORE_FLAGS: &[&str] = &[
    "for",
    "callers",
    "callees",
    "impact",
    "uses",
    "defs",
    "expand",
    "grep",
    "situ",
    "test-gate",
    "affected",
    "edit-check",
    "quality-delta",
    "quality-baseline",
    "help-task",
    "limit",
    "offset",
    "token-budget",
    "json",
];

const SHORT_HELP_FOOTER: &str = "Core Find and Check flags are listed. `--help` lists every flag (reports, lenses, edits, exports).";

/// The CLI with every plugin's flags and subcommands added, and non-core flags hidden from `-h`.
#[must_use]
pub fn augment(cmd: clap::Command) -> clap::Command {
    let cmd = REGISTRY.iter().fold(cmd, |cmd, command| {
        command
            .subcommands()
            .into_iter()
            .fold(command.augment(cmd), |cmd, sub| cmd.subcommand(sub))
    });
    let non_core: Vec<clap::Id> = cmd
        .get_arguments()
        .filter(|arg| {
            arg.get_long()
                .is_some_and(|long| !CORE_FLAGS.contains(&long))
        })
        .map(|arg| arg.get_id().clone())
        .collect();
    non_core
        .into_iter()
        .fold(cmd, |cmd, id| {
            cmd.mut_arg(id, |arg| arg.hide_short_help(true))
        })
        .after_help(SHORT_HELP_FOOTER)
}

/// Every registered command plugin, in dispatch order.
#[must_use]
pub fn commands() -> &'static [&'static dyn Command] {
    REGISTRY
}

/// Run the first plugin whose flags are present; warn when a shared option it does not honor was passed.
pub fn run(cli: &Cli, root: &str) -> Option<std::io::Result<()>> {
    REGISTRY.iter().find_map(|command| {
        let result = command.run(cli, root)?;
        if let Some(warning) = unhonored_option_warning(cli, *command) {
            eprintln!("{warning}");
        }
        Some(result)
    })
}

/// The warning for a shared option the command line carries but `command` does not declare.
#[must_use]
pub fn unhonored_option_warning(cli: &Cli, command: &dyn Command) -> Option<String> {
    let paged = cli.modifiers.limit.is_some() || cli.modifiers.offset.is_some();
    (paged
        && !command.name().is_empty()
        && !command.options().iter().any(|option| option.is_paged()))
    .then(|| {
        format!(
            "ripwire-rs: --limit/--offset are not honored by --{}",
            command.name()
        )
    })
}

/// Run the plugin subcommand the command line selected, if a plugin owns it.
pub fn run_subcommand(cli: &Cli, root: &str) -> Option<std::io::Result<()>> {
    let (name, matches) = cli.plugins.subcommand()?;
    REGISTRY
        .iter()
        .find_map(|command| command.run_subcommand(cli, name, matches, root))
}

/// Decode a plugin subcommand's own `Args` from the matches clap produced for it.
pub fn decode_subcommand<A: clap::FromArgMatches>(
    matches: &clap::ArgMatches,
) -> std::io::Result<A> {
    A::from_arg_matches(matches).map_err(std::io::Error::other)
}

/// Every plugin's MCP tools, sorted by name; each carries the parameters its command's shared options add.
pub fn tools() -> &'static [&'static CommandDescriptor] {
    static OWNED: std::sync::LazyLock<Vec<CommandDescriptor>> = std::sync::LazyLock::new(|| {
        let mut all: Vec<_> = REGISTRY
            .iter()
            .flat_map(|command| {
                command
                    .tools()
                    .iter()
                    .map(move |tool| plugin::with_shared_rows(command.options(), tool))
            })
            .collect();
        all.sort_by_key(|command| command.name);
        all
    });
    static TOOLS: std::sync::LazyLock<Vec<&'static CommandDescriptor>> =
        std::sync::LazyLock::new(|| OWNED.iter().collect());
    &TOOLS
}

/// Reject invalid flag combinations from every plugin.
pub fn validate_all(cli: &Cli) -> std::io::Result<()> {
    REGISTRY
        .iter()
        .try_for_each(|command| command.validate(cli))
}

/// Whether any plugin asks for a fresh clone of a remote checkout.
#[must_use]
pub fn refreshes_remote_checkout(cli: &Cli) -> bool {
    REGISTRY
        .iter()
        .any(|command| command.refreshes_remote_checkout(cli))
}

/// Run the default output, unless a plugin claims the command line first.
pub fn run_default(cli: &Cli, root: &str) -> std::io::Result<()> {
    REGISTRY
        .iter()
        .find_map(|command| command.run_default(cli, root))
        .unwrap_or(Ok(()))
}
