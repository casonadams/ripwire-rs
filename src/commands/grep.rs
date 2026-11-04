//! `--grep PAT`: literal trigram search, with the modifiers that refine its hits.
use super::common::grep::execute;
use super::common::mcp::mcp_tool;
use super::{Plugin, Run, SharedOption};
use crate::query::commands::{QueryContext, render_xml};
use crate::query::retrieval;
use crate::serialize::JsonValueExt;
use clap::Args;
use serde_json::Value;

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct GrepArgs {
    /// Literal trigram text search
    #[arg(long, value_name = "PAT")]
    pub grep: Option<String>,
}

fn run(run: &Run<'_>, args: &GrepArgs) -> std::io::Result<()> {
    let pat = args.grep.as_deref().unwrap_or_default();
    run.report(|ctx, out| {
        execute(
            ctx,
            pat,
            run.page(),
            run.cli.modifiers.in_scope.as_deref(),
            &run.cli.modifiers,
            out,
        )
    })
}

fn call_grep_tool(state: &QueryContext<'_>, args: &Value) -> Result<Value, String> {
    let corpus = state.sources.corpus();
    let req = retrieval::GrepRequest {
        corpus_paths: &corpus.rel_paths,
        pruned_dirs: corpus.pruned_dirs,
        limit: args.opt_usize("limit"),
        offset: args.opt_usize("offset").unwrap_or(0),
        path_filter: args.opt_str("path"),
        any_tier: crate::query::grep::parse_any_tier(args.opt_str("in"))
            .map_err(|error| error.to_string())?,
        before: args.opt_usize("before").unwrap_or(0),
        after: args.opt_usize("after").unwrap_or(0),
        graph: Some(state.graph),
        ..retrieval::GrepRequest::new(state.root, args.req_str("pattern")?, state.sources)
    };
    let val = retrieval::call_grep(state.ingest, state.interner, &req);
    render_xml(|out| crate::serialize::serialize_grep_xml(&val, out))
}

pub static GREP: Plugin<GrepArgs> = Plugin {
    name: "grep",
    options: &[SharedOption::PagedMatches],
    selected: |args| args.grep.is_some(),
    run,
    check: None,
    tools: &[mcp_tool!(
        "grep",
        "Literal search; each hit annotated with its enclosing symbol",
        &[
            ("pattern", "string", "Literal pattern to search"),
            ("path", "string", "Optional path filter"),
            ("in", "string", "Span tier: code (default) or any"),
            ("before", "integer", "Context lines before each match"),
            ("after", "integer", "Context lines after each match"),
        ],
        &["pattern"],
        call_grep_tool
    )],
};
