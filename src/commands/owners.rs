//! `--owners`: authorship and bus-factor analysis.
use super::common::mcp::mcp_tool;
use super::SharedOption;
use super::plugin::report_command;
use crate::commands::context::{ExecutionContext, Paginated};
use crate::commands::for_task;
use crate::git::history::{compute_git_at_stamp, resolve_cutoff_timestamp, walk_repository_history_scoped};
use crate::query::commands::{render_query_value, resolve_state_commits};
use crate::query::doc_tools;
use crate::serialize::{JsonValueExt, XmlWriter, serialize_owners_xml};
use serde_json::Value;
use std::collections::HashSet;
use std::io::{self, Write};

report_command! {
    name: "owners",
    args: OwnersArgs,
    static: OWNERS,
    options: [SharedOption::Paged],
    tools: [mcp_tool!(
        "owners",
        "Bus-factor: recency-weighted author ownership per file",
        &[
            ("path", "string", "Optional file path filter"),
            (
                "detail",
                "integer",
                "Show individual files when greater than zero",
            ),
        ],
        &[],
        |s, a| {
            let commits = resolve_state_commits(s, 12)?;
            let mut value = doc_tools::call_owners(
                Some(&commits),
                a.opt_str_any(&["path", "file"]),
                s.at,
                None,
            );
            value["root"] = s.root.into();
            value["detail"] = a
                .opt_usize("detail")
                .is_some_and(|detail| detail > 0)
                .into();
            render_query_value(value, a, serialize_owners_xml)
        }
    )],
    run: report(render),
    /// Authorship and bus-factor analysis
    #[arg(long)]
    owners: bool,
}

pub fn render<W: Write>(
    ctx: &ExecutionContext<'_>,
    _: &OwnersArgs,
    out: &mut XmlWriter<'_, W>,
) -> io::Result<()> {
    let cutoff = resolve_cutoff_timestamp(None, 12);
    let commits = walk_repository_history_scoped(ctx.root, cutoff).map_err(io::Error::other)?;
    let at_stamp = compute_git_at_stamp(ctx.root).map_err(io::Error::other)?;
    let known_files: HashSet<String> = ctx
        .ingest
        .files
        .iter()
        .map(|f| ctx.interner.resolve(&f.path).to_string())
        .collect();
    let mut val = doc_tools::call_owners(
        Some(commits.as_slice()),
        None,
        at_stamp.as_deref(),
        Some(&known_files),
    );
    val["root"] = Value::String(ctx.root.to_string());
    ctx.apply_pagination_to_json(&mut val);
    if for_task::detail(ctx.cli).is_some_and(|d| d > 0) {
        val["detail"] = Value::Bool(true);
    }
    serialize_owners_xml(&val, out)
}
