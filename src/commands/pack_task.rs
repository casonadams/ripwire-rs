//! `--pack-task TASK`: a single-call orientation bundle for a task, shaped to the token budget.
//! The partitioned variant lives in [`super::pack_task_partitioned`].
use super::common::retrieval::{report_query, token_budget};
use super::{Plugin, Run, SharedOption};
use crate::cli::Cli;
use crate::query::commands::QueryContext;
use crate::serialize::BYTES_PER_TOKEN;
use clap::Args;
use std::io::{self, Write};

/// The token ceiling a bundle takes when no budget flag is given.
pub(crate) const DEFAULT_PACK_TOKENS: usize = 6000;

pub fn execute<W: Write>(
    query: &QueryContext<'_>,
    task: &str,
    budget_tokens: usize,
    out: &mut W,
) -> io::Result<()> {
    crate::navigate::execute_pack_task(task, query, budget_tokens, out)
}

/// `--pack-task`.
#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct PackTaskArgs {
    /// Package single-call orientation task bundle under token budget
    #[arg(long = "pack-task", value_name = "TASK")]
    pub pack_task: Option<String>,
}

/// The token ceiling: `--token-budget` or `--max-tokens`, else `--pack-budget` (bytes), else the default.
fn budget_tokens(cli: &Cli) -> usize {
    token_budget(cli)
        .or(cli.pack_budget.map(|bytes| bytes / BYTES_PER_TOKEN))
        .unwrap_or(DEFAULT_PACK_TOKENS)
}

fn run(run: &Run<'_>, args: &PackTaskArgs) -> io::Result<()> {
    let task = args.pack_task.as_deref().unwrap_or_default();
    let tokens = budget_tokens(run.cli);
    report_query(run, |query, out| execute(query, task, tokens, out))
}

pub static PACK_TASK: Plugin<PackTaskArgs> = Plugin {
    name: "pack-task",
    options: &[SharedOption::Budget(
        "Token ceiling for the bundle (default 6000)",
    )],
    selected: |args| args.pack_task.is_some(),
    run,
    check: None,
    tools: &[],
};
