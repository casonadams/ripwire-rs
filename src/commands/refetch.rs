//! `--refetch`: clones a remote URL root fresh instead of reusing the cached clone.
use super::common::mcp::mcp_tool;
use super::{Command, SharedOption};
use crate::cli::Cli;
use crate::query::commands::{CommandDescriptor, QueryContext, render_xml};
use crate::serialize::JsonValueExt;
use clap::{Args, Command as ClapCommand};
use serde_json::Value;

/// `--refetch`.
#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct RefetchArgs {
    /// Clone a remote URL root fresh instead of reusing the cached clone
    #[arg(long = "refetch")]
    pub refetch: bool,
}

pub struct Refetch;
pub static REFETCH: Refetch = Refetch;

impl Command for Refetch {
    fn name(&self) -> &'static str {
        "refetch"
    }

    fn options(&self) -> &'static [SharedOption] {
        &[]
    }

    fn augment(&self, cmd: ClapCommand) -> ClapCommand {
        RefetchArgs::augment_args(cmd)
    }

    fn tools(&self) -> &'static [&'static CommandDescriptor] {
        TOOLS
    }

    fn refreshes_remote_checkout(&self, cli: &Cli) -> bool {
        cli.plugins
            .decode::<RefetchArgs>()
            .is_some_and(|args| args.refetch)
    }
}

fn call_body(context: &QueryContext<'_>, args: &Value) -> Result<Value, String> {
    let request = crate::query::detail::BodyRequest {
        root: context.root,
        symbols: args.req_str("symbol")?,
        path_hint: args.opt_str("path"),
        start_line: args.opt_usize("start_line"),
        end_line: args.opt_usize("end_line"),
        outline: args.bool_or("outline", false),
        compress: args.bool_or("compress", false),
    };
    render_xml(|out| {
        crate::query::detail::execute_body_query(
            &request,
            context.ingest,
            context.interner,
            context.sources,
            out,
        )
    })
}

static TOOLS: &[&CommandDescriptor] = &[mcp_tool!(
    "fetch_body",
    "Full (or partial-range) source of a symbol's definition",
    &[
        ("symbol", "string", "Symbol name"),
        ("path", "string", "Optional file path hint"),
        ("start_line", "integer", "Optional 1-based start line"),
        ("end_line", "integer", "Optional 1-based end line"),
        ("outline", "boolean", "Show definition skeleton"),
        ("compress", "boolean", "Compress implementation source"),
    ],
    &["symbol"],
    call_body
)];
