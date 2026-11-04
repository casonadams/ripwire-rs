//! `--doc-drift`: detect stale doc references against the live index.
use super::common::mcp::mcp_tool;
use super::{Plugin, Run, SharedOption};
use crate::commands::context::Paginated;
use crate::query::FileSources;
use crate::query::doc_tools;
use crate::query::commands::render_query_value;
use crate::serialize::{JsonValueExt, serialize_doc_drift_xml};
use clap::Args;
use std::io;
use std::path::Path;

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct DocDriftArgs {
    /// Detect stale doc references
    #[arg(long = "doc-drift")]
    pub doc_drift: bool,
}

pub static DOC_DRIFT: Plugin<DocDriftArgs> = Plugin {
    name: "doc-drift",
    options: &[SharedOption::CliPaged],
    selected: |args| args.doc_drift,
    run,
    check: None,
    tools: &[mcp_tool!(
        "doc_drift",
        "WHICH OF THIS REPO'S DOC CLAIMS ARE NOW FALSE: verifies checkable anchors in markdown against live index",
        &[("with_history", "boolean", "Include repository history")],
        &[],
        |s, a| {
            let value = doc_tools::doc_drift_with_sources_and_history(
                s.root,
                s.sources,
                s.ingest,
                s.interner,
                a.bool_or("with_history", false),
            );
            render_query_value(value, a, serialize_doc_drift_xml)
        }
    )],
};

fn run(run: &Run<'_>, _: &DocDriftArgs) -> io::Result<()> {
    run.report(|ctx, out| {
        let sources = FileSources {
            root: Path::new(ctx.root),
        };
        let mut val = doc_tools::doc_drift_with_sources_and_history(
            ctx.root,
            &sources,
            ctx.ingest,
            ctx.interner,
            ctx.cli.modifiers.with_history,
        );
        ctx.apply_pagination_to_json(&mut val);
        serialize_doc_drift_xml(&val, out)
    })
}
