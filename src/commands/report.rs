//! `--report`: executive markdown architecture report (always plain markdown).
use super::common::safety::context_of;
use super::{Plugin, Run, SharedOption};
use crate::commands::context::Paginated;
use crate::commands::context::ExecutionContext;
use crate::serialize::XmlWriter;
use clap::Args;
use std::io::{self, Write};

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct ReportArgs {
    /// Generate executive markdown architecture report
    #[arg(long = "report")]
    pub report: bool,
}

/// The report's byte budget when no token or pack budget is given: a cold-start orientation, not an inventory.
const DEFAULT_REPORT_BYTES: usize = 16 * 1024;

pub fn render(ctx: &ExecutionContext<'_>, out: &mut XmlWriter<'_, impl Write>) -> io::Result<()> {
    let cli = ctx.cli;
    let mut page = cli.page_request();
    if cli.token_budget.is_none() && cli.pack_budget.is_none() {
        page.budget = Some(DEFAULT_REPORT_BYTES);
    }
    crate::metrics::serialize_executive_report_md(&context_of(ctx), &ctx.pr.ranks, page, out)
}

fn run(run: &Run<'_>, _args: &ReportArgs) -> io::Result<()> {
    run.report_plain(render)
}

pub static REPORT: Plugin<ReportArgs> = Plugin {
    name: "report",
    options: &[SharedOption::CliPaged],
    selected: |args| args.report,
    run,
    check: None,
    tools: &[],
};
