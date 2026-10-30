//! `--situ [FILES]`: blast radius, tests, and co-change for a change (default: git diff); MCP `situational_awareness`.
use super::common::mcp::mcp_tool;
use super::common::safety_queries::execute_situ_query;
use super::{Plugin, Run};
use crate::query::commands::{render_xml, resolve_state_commits};
use crate::serialize::JsonValueExt;
use clap::Args;
use std::io;

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct SituArgs {
    /// Situational awareness for a change: blast radius + tests + co-change (default: git diff)
    #[arg(long, value_name = "FILES")]
    pub situ: Option<Option<String>>,
}

fn run(run: &Run<'_>, args: &SituArgs) -> io::Result<()> {
    let files = args.situ.clone().flatten();
    run.report(|ctx, out| {
        let safety = crate::query::safety::SafetyContext {
            root: ctx.root,
            ingest: ctx.ingest,
            graph: ctx.graph,
            interner: ctx.interner,
        };
        execute_situ_query(&safety, files.as_deref(), run.page(), out)
    })
}

pub static SITU: Plugin<SituArgs> = Plugin {
    name: "situ",
    options: &[],
    selected: |args| args.situ.is_some(),
    run,
    check: None,
    tools: &[mcp_tool!(
        "situational_awareness",
        "The 5 things to know about a diff: blast_radius, tests_to_run, forgotten co-change files, hotspots",
        &[(
            "files",
            "string[]",
            "Changed file paths; omitted means the working-tree diff against git HEAD"
        )],
        &[],
        |s, a| {
            let commits = resolve_state_commits(s, 18);
            let files = if a.get("files").is_some() {
                a.str_vec("files")
            } else {
                super::common::changes::resolve_changed_or_spec_files(s.root, None)
            };
            let report = crate::query::safety::SituationalReport {
                root: s.root,
                files: &files,
                commits: Some(&commits),
                page: crate::serialize::PageRequest::default(),
                context: crate::query::safety::SafetyContext {
                    root: s.root,
                    ingest: s.ingest,
                    graph: s.graph,
                    interner: s.interner,
                },
            };
            use crate::serialize::XmlReport;
            render_xml(|out| report.render_envelope(out))
        }
    )],
};
