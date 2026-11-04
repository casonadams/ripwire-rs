//! `--graph-query EXPR`: evaluates the functional graph query DSL over the call graph.
use crate::commands::common::retrieval::report_text_query;
use crate::query::commands::QueryContext;
use crate::serialize::PageRequest;
use super::{Plugin, Run, SharedOption};
use clap::Args;

/// `--graph-query`.
#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct GraphQueryArgs {
    /// Evaluate small functional graph query DSL expression
    #[arg(long = "graph-query", value_name = "EXPR")]
    pub graph_query: Option<String>,
}

pub fn execute<W: std::io::Write>(
    query: &QueryContext<'_>,
    expr: &str,
    page: PageRequest,
    out: &mut W,
) -> std::io::Result<()> {
    crate::navigate::execute_graph_query_expr_paged(
        expr,
        query.root,
        query.ingest,
        query.graph,
        query.interner,
        page,
        out,
    )
}

fn run(run: &Run<'_>, args: &GraphQueryArgs) -> std::io::Result<()> {
    report_text_query(run, args.graph_query.as_deref(), |query, expr, page, out| {
        execute(query, expr, page, out)
    })
}

pub static GRAPH_QUERY: Plugin<GraphQueryArgs> = Plugin {
    name: "graph-query",
    options: &[SharedOption::Paged],
    selected: |args| args.graph_query.is_some(),
    run,
    check: None,
    tools: &[],
};
