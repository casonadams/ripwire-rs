//! `--plan-lanes [N]`: plans parallel worktree lanes before code (split across N worktrees).
use super::common::safety::context_of;
use super::common::safety_queries::compute_plan_lanes_from_facts;
use super::{Plugin, Run};
use clap::Args;
use std::io::Write;
use std::io;

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct PlanLanesArgs {
    /// Plan parallel worktree lanes before code (split across N worktrees)
    #[arg(long = "plan-lanes", num_args = 0..=1, default_missing_value = "2", value_name = "N")]
    pub plan_lanes: Option<Option<usize>>,
    /// Explicit per-lane brief file for --plan-lanes (one lane task per line)
    #[arg(long = "brief", num_args = 0..=1, default_missing_value = "", value_name = "FILE")]
    pub brief: Option<String>,
}

fn run(run: &Run<'_>, args: &PlanLanesArgs) -> io::Result<()> {
    let requested = args.plan_lanes.flatten().unwrap_or(2);
    let task = run
        .cli
        .modifiers
        .task
        .clone()
        .unwrap_or_else(|| "general refactoring".to_string());
    let brief_lines = args
        .brief
        .as_deref()
        .filter(|p| !p.is_empty())
        .map(|p| read_brief(p, std::fs::read_to_string(p)))
        .transpose()?;
    run.report(|ctx, out| {
        let safety = context_of(ctx);
        let val = compute_plan_lanes_from_facts(&safety, requested, &task, brief_lines.as_deref(), run.page());
        serde_json::to_writer(&mut *out, &val)?;
        writeln!(out)
    })
}

/// Brief lines from an explicit `--brief` file; a read error is reported, never an empty brief.
pub fn read_brief(path: &str, read: io::Result<String>) -> io::Result<Vec<String>> {
    read.map(|c| c.lines().map(str::to_string).collect())
        .map_err(|e| io::Error::new(e.kind(), format!("{path}: {e}")))
}

pub static PLAN_LANES: Plugin<PlanLanesArgs> = Plugin {
    name: "plan-lanes",
    options: &[],
    selected: |args| args.plan_lanes.is_some(),
    run,
    check: None,
    tools: &[],
};
