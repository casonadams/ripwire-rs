//! `--cochange [FILE]` / `--cochange-groups`: Fowler's Shotgun Surgery co-change mining; MCP `cochange`.
use super::common::history::execute_with_git_state_fmt;
use super::common::mcp::mcp_tool;
use super::common::safety::safety_context;
use super::context::OutputDialect;
use super::{Plugin, Run, SharedOption};
use crate::query::commands::{QueryContext, render_xml, resolve_state_commits};
use crate::serialize::{JsonValueExt, PageRequest};
use clap::Args;
use serde_json::Value;
use std::io;

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct CochangeArgs {
    /// Fowler's Shotgun Surgery co-change mining
    #[arg(long)]
    pub cochange: Option<Option<String>>,
    /// Limit git history traversal to commits after this date or revision
    #[arg(long)]
    pub since: Option<String>,
}

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct CochangeGroupArgs {
    /// Cluster co-changing files into multi-file change groups
    #[arg(long = "cochange-groups")]
    pub cochange_groups: bool,
    /// Minimum sub-window recurrence count for co-change pairs
    #[arg(long = "cochange-recur", value_name = "K")]
    pub cochange_recur: Option<u32>,
}

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct CochangeFlags {
    #[command(flatten)]
    pub cochange: CochangeArgs,
    #[command(flatten)]
    pub groups: CochangeGroupArgs,
}

/// One co-change query: optional file, history window, grouping, paging, and output shape.
pub struct CochangeQuery<'a> {
    pub file: Option<&'a str>,
    pub since: Option<&'a str>,
    pub recur: Option<u32>,
    pub groups: bool,
    pub page: PageRequest,
    pub pretty: bool,
    pub json_out: bool,
}

/// Runs a co-change query over an 18-month git window rooted at `root`.
pub fn execute_cochange(root: &str, query: CochangeQuery<'_>) -> io::Result<()> {
    let cutoff = crate::git::history::resolve_cutoff_timestamp(query.since, 18);
    execute_with_git_state_fmt(
        root,
        cutoff,
        query.pretty,
        query.json_out,
        |commits, state, out| {
            let options = crate::query::safety::CochangeOptions {
                since: query.since,
                cochange_recur: query.recur,
                cochange_groups: query.groups,
                page: query.page,
            };
            crate::query::safety::execute_cochange_with_options(
                query.file,
                options,
                commits,
                &state.safety_context(),
                out,
            )
        },
    )
}

fn run(run: &Run<'_>, args: &CochangeFlags) -> io::Result<()> {
    execute_cochange(
        run.root,
        CochangeQuery {
            file: args.cochange.cochange.as_ref().and_then(|f| f.as_deref()),
            since: args.cochange.since.as_deref(),
            recur: args.groups.cochange_recur,
            groups: args.groups.cochange_groups,
            page: run.page(),
            pretty: run.cli.is_pretty_out(),
            json_out: run.cli.is_json_out(),
        },
    )
}

fn call_cochange(state: &QueryContext<'_>, args: &Value) -> Result<Value, String> {
    let commits = resolve_state_commits(state, crate::git::history::DEFAULT_COCHANGE_MONTHS)?;
    let context = safety_context(state);
    let options = crate::query::safety::CochangeOptions {
        since: args.opt_str("since"),
        cochange_recur: args.opt_usize("recur").map(|value| value as u32),
        cochange_groups: args.bool_or("groups", false),
        page: crate::serialize::PageRequest {
            limit: args.opt_usize("limit"),
            offset: args.opt_usize("offset"),
            ..crate::serialize::PageRequest::default()
        },
    };
    render_xml(|out| {
        crate::query::safety::execute_cochange_with_options(
            args.opt_str("path"),
            options,
            &commits,
            &context,
            out,
        )
    })
}

pub static COCHANGE: Plugin<CochangeFlags> = Plugin {
    name: "cochange",
    options: &[SharedOption::Paged],
    selected: |args| args.cochange.cochange.is_some() || args.groups.cochange_groups,
    run,
    check: None,
    tools: &[mcp_tool!(
        "cochange",
        "Files that historically change together with this file (Fowler's Shotgun Surgery logical coupling)",
        &[
            (
                "path",
                "string",
                "Target file path to find co-change partners for"
            ),
            (
                "since",
                "string",
                "Time window or revision (e.g. '18 months ago')"
            ),
            ("recur", "integer", "Recursive co-change depth"),
            ("groups", "boolean", "Report co-change groups"),
        ],
        &[],
        call_cochange
    )],
};
