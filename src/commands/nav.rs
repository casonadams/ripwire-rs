//! Shared by the navigation commands (`callers`, `callees`, `impact`, `uses`) and the
//! `navigate` subcommand.
use super::{Command, Run, SharedOption, decode_subcommand};
use crate::cli::Cli;
use crate::commands::context::{OutputDialect, Paginated, load_repo_analysis_for_cli};
use crate::query::commands::{QueryContext, render_xml};
use crate::query::navigation::{NavContext, NavMode, execute_nav_symbol_query_in_root};
use crate::serialize::JsonValueExt;
use clap::{Args, Command as ClapCommand};
use serde_json::Value;
use std::io::{self, Write};

/// `navigate` subcommand: symbol navigation with its target as a positional argument.
#[derive(Args, Debug, Default, PartialEq)]
pub struct NavigateArgs {
    /// Target repository path
    #[arg(value_name = "PATH")]
    pub path: Option<String>,
    /// Target symbol name
    #[arg(short, long)]
    pub symbol: Option<String>,
    /// Show direct 1-hop callers
    #[arg(long)]
    pub callers: bool,
    /// Show direct 1-hop callees
    #[arg(long)]
    pub callees: bool,
    /// Show transitive blast radius
    #[arg(long)]
    pub impact: bool,
}

/// Run one navigation query for the selected command, honoring its shared options.
pub fn run_nav(run: &Run<'_>, symbol: &str, mode: NavMode) -> io::Result<()> {
    run.report(|ctx, out| {
        let context = NavContext {
            ingest: ctx.ingest,
            graph: ctx.graph,
            ranks: &ctx.pr.ranks,
            pr_iters: Some(ctx.pr.iterations),
            interner: ctx.interner,
            page: run.page(),
            max_depth: run.cli.modifiers.impact_depth,
        };
        execute_nav_symbol_query_in_root(run.root, symbol, mode, &context, out)
    })
}

pub struct Nav;
pub static NAV: Nav = Nav;

impl Command for Nav {
    fn name(&self) -> &'static str {
        "navigate"
    }

    fn options(&self) -> &'static [SharedOption] {
        &[SharedOption::Paged]
    }

    fn subcommands(&self) -> Vec<ClapCommand> {
        vec![
            NavigateArgs::augment_args(ClapCommand::new("navigate"))
                .about("Code navigation and call graph traversals (callers, callees, impact)"),
        ]
    }

    fn run_subcommand(
        &self,
        cli: &Cli,
        name: &str,
        matches: &clap::ArgMatches,
        root: &str,
    ) -> Option<io::Result<()>> {
        if name != "navigate" {
            return None;
        }
        Some(
            decode_subcommand::<NavigateArgs>(matches)
                .and_then(|args| execute_navigate(cli, &args, root)),
        )
    }
}

/// MCP handler shared by the navigation tools.
pub fn call_navigation(
    state: &QueryContext<'_>,
    args: &Value,
    mode: NavMode,
) -> Result<Value, String> {
    let symbol = args.req_str("symbol")?;
    let selector = args
        .opt_str("file")
        .map(|file| format!("{file}::{symbol}"));
    let symbol = selector.as_deref().unwrap_or(symbol);
    let context = NavContext {
        ingest: state.ingest,
        graph: state.graph,
        ranks: &state.pr.ranks,
        pr_iters: Some(state.pr.iterations),
        interner: state.interner,
        page: crate::serialize::PageRequest {
            limit: args.opt_usize("limit"),
            offset: args.opt_usize("offset"),
            budget: args.opt_usize("budget_tokens").map(crate::query::map::token_budget_bytes),
        },
        max_depth: args.opt_usize("max_depth").map(|depth| depth as u32),
    };
    render_xml(|out| execute_nav_symbol_query_in_root(state.root, symbol, mode, &context, out))
}

/// An MCP navigation tool descriptor.
macro_rules! nav_tool {
    ($name:expr, $desc:expr, $mode:expr) => {
        &$crate::query::commands::CommandDescriptor {
            name: $name,
            description: $desc,
            params: std::borrow::Cow::Borrowed(&[
                (
                    "symbol",
                    "string",
                    "Name to look up: bare (run), owner-qualified (Owner::run or Owner.run), or a definition's location (src/app.rs:42)",
                ),
                (
                    "file",
                    "string",
                    "Path of the file defining the symbol, to pick one definition when several share the name",
                ),
                ("max_depth", "integer", "Maximum traversal depth"),
            ]),
            required: &["symbol"],
            handler: |state, args| $crate::commands::nav::call_navigation(state, args, $mode),
        }
    };
}
pub(crate) use nav_tool;

pub fn execute_navigate_with<W: Write>(
    root: &str,
    args: &NavigateArgs,
    ctx: &NavContext<'_>,
    out: &mut W,
) -> io::Result<()> {
    let sym_name = args.symbol.as_deref().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "--symbol is required for navigate",
        )
    })?;
    let mode = if args.impact {
        NavMode::Impact
    } else if args.callers {
        NavMode::Callers
    } else if args.callees {
        NavMode::Callees
    } else {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "navigate needs one of --callers, --callees, --impact",
        ));
    };
    execute_nav_symbol_query_in_root(root, sym_name, mode, ctx, out)
}

pub fn execute_navigate(cli: &Cli, args: &NavigateArgs, default_root: &str) -> io::Result<()> {
    let root = args.path.as_deref().unwrap_or(default_root);
    let (ingest, graph, pr, interner) = load_repo_analysis_for_cli(cli, root, false)?;
    let ctx = NavContext {
        ingest: &ingest,
        graph: &graph,
        ranks: &pr.ranks,
        pr_iters: Some(pr.iterations),
        interner: &interner,
        page: cli.page_request(),
        max_depth: cli.modifiers.impact_depth,
    };
    let mut stdout_lock = io::stdout().lock();
    let mut stdout = cli.wrap_xml_writer(&mut stdout_lock, false);
    execute_navigate_with(root, args, &ctx, &mut stdout)?;
    stdout.finish()
}
