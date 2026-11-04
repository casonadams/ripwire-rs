//! `--for TASK` (alias `--query`): task-lens ranked building blocks; owns the flags that shape that bundle.
use super::common::mcp::mcp_tool;
use super::common::retrieval::{report_query, token_budget};
use super::{Plugin, Run, SharedOption};
use crate::cli::Cli;
use crate::query::commands::{QueryContext, render_query_value};
use crate::query::retrieval::{self, ForTaskOptions};
use crate::serialize::{JsonValueExt, PageRequest};
use clap::Args;
use serde_json::Value;
use std::io::{self, Write};

/// The `--for` token ceiling when no budget flag is given: a bounded reading order, not an inventory.
const DEFAULT_FOR_TOKENS: usize = 4000;

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct ForTaskArgs {
    /// Importance-weighted detail: full bodies for top N symbols, or lift collapsed rows
    #[arg(long = "detail", num_args = 0..=1, default_missing_value = "1", value_name = "N")]
    pub detail: Option<usize>,
    /// Cut returned set at the relevance cliff instead of fixed top-k
    #[arg(long = "adaptive")]
    pub adaptive: bool,
    /// Restore automatic body walk on the conceptual route
    #[arg(long = "auto-bodies")]
    pub auto_bodies: bool,
    /// Opt in to full lego and compose sections (lego,compose)
    #[arg(long = "sections", value_name = "SECTIONS")]
    pub sections: Option<String>,
    /// Ranked signatures and bodies for a task or a symbol name; a name gives its callers too
    #[arg(long = "for", visible_alias = "query", value_name = "TASK")]
    pub task_for: Option<String>,
    /// Emit only signatures without bodies
    #[arg(long = "signatures-only")]
    pub signatures_only: bool,
}

impl ForTaskArgs {
    /// The `for` options for `query`, scoped by `in_scope`, capped by `budget` and windowed by `page`.
    pub fn options<'a>(
        &'a self,
        query: &QueryContext<'a>,
        in_scope: Option<&'a str>,
        budget: Option<usize>,
        page: PageRequest,
    ) -> ForTaskOptions<'a> {
        ForTaskOptions {
            root: query.root,
            at: query.at,
            graph: Some(query.graph),
            sources: Some(query.sources),
            budget_tokens: budget,
            limit: page.limit,
            offset: page.offset,
            in_scope,
            sections: self.sections.as_deref(),
            signatures_only: self.signatures_only,
            auto_bodies: self.auto_bodies,
            detail: self.detail,
            adaptive: self.adaptive,
        }
    }
}

/// The `--detail` count, shared with the workspace commands that take it.
pub fn detail(cli: &Cli) -> Option<usize> {
    cli.plugins.decode::<ForTaskArgs>()?.detail
}

/// Writes the task bundle for `task`, failing with the bundle's error message when it has one.
pub fn execute<W: Write>(
    query: &QueryContext<'_>,
    task: &str,
    args: &ForTaskArgs,
    in_scope: Option<&str>,
    budget: Option<usize>,
    page: PageRequest,
    out: &mut W,
) -> io::Result<()> {
    let val = retrieval::call_for_task_with_options(
        query.ingest,
        query.interner,
        &query.pr.ranks,
        task,
        args.options(query, in_scope, budget, page),
    );
    if let Some(error) = val.get("error").and_then(Value::as_str) {
        return Err(io::Error::other(error.to_owned()));
    }
    crate::serialize::serialize_for_xml(&val, out)
}

/// The task bundle for `task` with the flags of `args` (used by the orientation report).
pub fn orient_bundle(cli: &Cli, query: &QueryContext<'_>, task: &str) -> Value {
    let args = cli.plugins.decode::<ForTaskArgs>().unwrap_or_default();
    retrieval::call_for_task_with_options(
        query.ingest,
        query.interner,
        &query.pr.ranks,
        task,
        args.options(
            query,
            cli.modifiers.in_scope.as_deref(),
            token_budget(cli),
            PageRequest::default(),
        ),
    )
}

fn run(run: &Run<'_>, args: &ForTaskArgs) -> io::Result<()> {
    let task = args.task_for.as_deref().unwrap_or_default();
    let in_scope = run.cli.modifiers.in_scope.as_deref();
    let budget = Some(token_budget(run.cli).unwrap_or(DEFAULT_FOR_TOKENS));
    let page = run.page();
    report_query(run, |query, out| {
        execute(query, task, args, in_scope, budget, page, out)
    })
}

fn check(args: &ForTaskArgs) -> io::Result<()> {
    let Some(raw) = args.sections.as_deref() else {
        return Ok(());
    };
    if args.task_for.is_none() {
        return Err(io::Error::other(
            "ripwire-rs: --sections= modifies --for=TASK; pass it too",
        ));
    }
    retrieval::parse_for_sections(raw)
        .map(|_| ())
        .map_err(io::Error::other)
}

fn call_for_tool(state: &QueryContext<'_>, args: &Value) -> Result<Value, String> {
    let value = retrieval::call_for_task_with_options(
        state.ingest,
        state.interner,
        &state.pr.ranks,
        args.req_str("task")?,
        ForTaskOptions {
            root: state.root,
            at: state.at,
            graph: Some(state.graph),
            sources: Some(state.sources),
            budget_tokens: args.opt_usize("budget_tokens"),
            limit: None,
            offset: None,
            detail: args.opt_usize("detail"),
            adaptive: args.bool_or("adaptive", false),
            in_scope: args.opt_str("in_scope"),
            sections: args.opt_str("sections"),
            signatures_only: args.bool_or("signatures_only", false),
            auto_bodies: args.bool_or("auto_bodies", false),
        },
    );
    render_query_value(value, args, crate::serialize::serialize_for_xml)
}

pub static FOR_TASK: Plugin<ForTaskArgs> = Plugin {
    name: "for",
    options: &[
        SharedOption::Budget("Approximate token ceiling"),
        SharedOption::CliPaged,
    ],
    selected: |args| args.task_for.is_some(),
    run,
    check: Some(check),
    tools: &[mcp_tool!(
        "for",
        "Task-lens ranked signatures-only inventory of building blocks most relevant to a task",
        &[
            ("task", "string", "Task description in plain words"),
            ("detail", "integer", "Maximum implementation bodies"),
            ("adaptive", "boolean", "Adapt detail to task confidence"),
            ("in_scope", "string", "Path filter applied before ranking"),
            ("sections", "string", "Comma-separated report sections"),
            ("signatures_only", "boolean", "Omit implementation bodies"),
            (
                "auto_bodies",
                "boolean",
                "Include high-value implementation bodies"
            ),
        ],
        &["task"],
        call_for_tool
    )],
};
