//! `--slice SYM[:VAR]` and the `slice` subcommand: ARISE intra-procedural def-use variable slice; MCP `slice`.
use super::common::mcp::mcp_tool;
use super::{Command, SharedOption, run_named_subcommand};
use crate::cli::Cli;
use crate::commands::context::{OutputDialect, load_repo_analysis_for_cli};
use crate::ingest::LanguageRegistry;
use crate::query::commands::{CommandDescriptor, QueryContext};
use crate::serialize::JsonValueExt;
use clap::{Args, Command as ClapCommand};
use serde_json::Value;
use std::io::{self, Write};
use std::path::Path;

/// Top-level ARISE variable slice flags.
#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct SliceFlags {
    /// ARISE intra-procedural def-use variable slice (SYM[:VAR])
    #[arg(long)]
    pub slice: Option<String>,

    /// Dataflow direction for --slice (fwd, back, both)
    #[arg(long, default_value = "fwd")]
    pub slice_flow: String,

    /// Max hop depth for --slice
    #[arg(long = "slice-depth", default_value_t = 8)]
    pub slice_depth: u32,
}

/// `slice` subcommand: a variable slice with its target as a positional argument.
#[derive(Args, Debug, Default, PartialEq)]
pub struct SliceArgs {
    /// Target variable criterion in symbol (e.g. sym:var)
    #[arg(value_name = "TARGET")]
    pub target: Option<String>,

    /// Dataflow direction (fwd, back, both)
    #[arg(long, default_value = "fwd")]
    pub flow: String,
}

fn execute_slice_for_cli(
    cli: &Cli,
    root: &str,
    target: Option<&str>,
    flow: &str,
    max_depth: u32,
) -> io::Result<()> {
    let target = target
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "slice target is required"))?;
    let flow = flow
        .parse()
        .map_err(|e: String| io::Error::new(io::ErrorKind::InvalidInput, e))?;
    let (ingest, _, _, interner) = load_repo_analysis_for_cli(cli, root, false)?;
    let registry = LanguageRegistry::new().map_err(io::Error::other)?;
    let sources = crate::query::FileSources {
        root: Path::new(root),
    };
    let at = crate::git::history::compute_git_at_stamp(root).map_err(io::Error::other)?;
    let report = crate::slice::query_slice(
        crate::slice::SliceRequest {
            root,
            target,
            flow,
            max_depth,
            at: at.as_deref(),
        },
        crate::slice::SliceContext {
            ingest: &ingest,
            interner: &interner,
            registry: &registry,
        },
        &sources,
    )
    .map_err(io::Error::other)?;
    let mut stdout_lock = io::stdout().lock();
    let mut stdout = cli.wrap_xml_writer(&mut stdout_lock, false);
    stdout.write_all(report.as_bytes())?;
    stdout.finish()
}

fn call_slice(state: &QueryContext<'_>, args: &Value) -> Result<Value, String> {
    let registry = LanguageRegistry::new().map_err(|error| error.to_string())?;
    let request = crate::slice::SliceRequest {
        root: state.root,
        target: args.req_str("target")?,
        at: state.at,
        flow: args
            .str_or("flow", "fwd")
            .parse::<crate::slice::SliceFlowDir>()?,
        max_depth: args.opt_usize("max_depth").unwrap_or(8) as u32,
    };
    let context = crate::slice::SliceContext {
        ingest: state.ingest,
        interner: state.interner,
        registry: &registry,
    };
    crate::slice::query_slice(request, context, state.sources).map(Value::String)
}

static TOOLS: &[&CommandDescriptor] = &[mcp_tool!(
    "slice",
    "ARISE intra-procedural def-use variable dataflow slice inside a function",
    &[
        ("target", "string", "sym:var or bare sym to list locals"),
        ("flow", "string", "fwd, back, or both"),
        ("max_depth", "integer", "Maximum dataflow depth (default 8)"),
    ],
    &["target"],
    call_slice
)];

pub struct Slice;

pub static SLICE: Slice = Slice;

impl Command for Slice {
    fn name(&self) -> &'static str {
        "slice"
    }

    fn options(&self) -> &'static [SharedOption] {
        &[]
    }

    fn augment(&self, cmd: ClapCommand) -> ClapCommand {
        SliceFlags::augment_args(cmd)
    }

    fn subcommands(&self) -> Vec<ClapCommand> {
        vec![
            SliceArgs::augment_args(ClapCommand::new("slice"))
                .about("ARISE intra-procedural def-use variable slicing"),
        ]
    }

    fn run(&self, cli: &Cli, root: &str) -> Option<io::Result<()>> {
        let flags = cli.plugins.decode::<SliceFlags>()?;
        let target = flags.slice.as_deref()?;
        Some(execute_slice_for_cli(
            cli,
            root,
            Some(target),
            &flags.slice_flow,
            flags.slice_depth,
        ))
    }

    fn run_subcommand(
        &self,
        cli: &Cli,
        name: &str,
        matches: &clap::ArgMatches,
        root: &str,
    ) -> Option<io::Result<()>> {
        run_named_subcommand::<SliceArgs>(name, "slice", matches, |args| {
            let depth = cli.plugins.decode::<SliceFlags>().unwrap_or_default().slice_depth;
            execute_slice_for_cli(cli, root, args.target.as_deref(), &args.flow, depth)
        })
    }

    fn tools(&self) -> &'static [&'static CommandDescriptor] {
        TOOLS
    }
}
