//! `--communities` and `--community ID`: Leiden module clustering of the call graph.
use super::{Plugin, Run, SharedOption};
use crate::commands::context::{ExecutionContext, Paginated};
use crate::domain::{IngestResult, Interner};
use crate::graph::Graph;
use crate::metrics::architecture::{build_ranked_communities, compute_community_summary};
use crate::serialize::{PageRequest, XmlWriter};
use clap::Args;
use std::io::{self, Write};

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct CommunitiesArgs {
    /// Cluster call graph into cohesive modules (Leiden community detection)
    #[arg(long)]
    pub communities: bool,
    /// Inspect members and bridges of one specific community module
    #[arg(long, value_name = "ID")]
    pub community: Option<u32>,
}

pub fn execute_communities_query<W: Write>(
    root: &str,
    target_id: Option<u32>,
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    page: PageRequest,
    out: &mut W,
) -> io::Result<()> {
    let (comms, by_comm, modules) = build_ranked_communities(target_id, ingest, graph, interner);
    let summary = compute_community_summary(&comms, &by_comm, &modules, ingest, graph);
    crate::serialize::serialize_communities_with_summary_xml(root, &modules, &summary, page, out)
}

pub fn render<W: Write>(
    ctx: &ExecutionContext<'_>,
    args: &CommunitiesArgs,
    out: &mut XmlWriter<'_, W>,
) -> io::Result<()> {
    execute_communities_query(
        ctx.root,
        args.community,
        ctx.ingest,
        ctx.graph,
        ctx.interner,
        ctx.cli.page_request(),
        out,
    )
}

fn run(run: &Run<'_>, args: &CommunitiesArgs) -> io::Result<()> {
    run.report(|ctx, out| render(ctx, args, out))
}

pub static COMMUNITIES: Plugin<CommunitiesArgs> = Plugin {
    name: "communities",
    options: &[SharedOption::Paged],
    selected: |args| args.communities || args.community.is_some(),
    run,
    check: None,
    tools: &[],
};
