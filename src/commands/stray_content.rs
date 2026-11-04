//! `--stray-content`: lines authored on divergent branches that HEAD does not have.
use super::common::mcp::mcp_tool;
use super::{Plugin, Run, SharedOption};
use crate::query::commands::render_query_value;
use crate::query::doc_tools;
use crate::serialize::serialize_stray_content_xml;
use clap::Args;
use std::io;

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct StrayContentArgs {
    /// Divergent branch content tracking
    #[arg(long = "stray-content")]
    pub stray_content: bool,
    /// Generate landing plan in stray-content
    #[arg(long = "plan")]
    pub plan: bool,
}

pub static STRAY_CONTENT: Plugin<StrayContentArgs> = Plugin {
    name: "stray-content",
    options: &[SharedOption::McpPaged],
    selected: |args| args.stray_content,
    run,
    check: None,
    tools: &[mcp_tool!(
        "stray_content",
        "Lines authored on divergent branches that HEAD does not have",
        &[
        ],
        &[],
        |s, a| render_query_value(doc_tools::call_stray_content(s.root)?, a, serialize_stray_content_xml)
    )],
};

fn run(run: &Run<'_>, _: &StrayContentArgs) -> io::Result<()> {
    run.report(|ctx, out| {
        let value = doc_tools::call_stray_content(ctx.root).map_err(io::Error::other)?;
        serialize_stray_content_xml(&value, out)
    })
}
