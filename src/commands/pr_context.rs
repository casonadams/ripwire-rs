//! `--pr-context [REF]`: review bundle per changed file vs base (symbols, callers, blast radius, tests).
use super::common::safety::context_of;
use super::common::safety_queries::execute_pr_context_dispatch;
use super::{Plugin, Run};
use clap::Args;
use std::io;

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct PrContextArgs {
    /// Review bundle per changed file vs base: symbols, callers, blast radius, tests
    #[arg(long = "pr-context", value_name = "REF")]
    pub pr_context: Option<Option<String>>,
}

/// The page with a `--token-budget` / `--max-tokens` request converted at
/// [`crate::serialize::BYTES_PER_TOKEN`], so `budget_tokens=` reports the tokens asked for.
fn pr_context_page(run: &Run<'_>) -> crate::serialize::PageRequest {
    let page = run.page();
    match run.cli.token_budget.or(run.cli.max_tokens) {
        Some(tokens) => page.with_budget(Some(
            tokens.saturating_mul(crate::serialize::BYTES_PER_TOKEN),
        )),
        None => page,
    }
}

fn run(run: &Run<'_>, args: &PrContextArgs) -> io::Result<()> {
    let spec = args.pr_context.clone().flatten();
    let reference = spec
        .as_deref()
        .map(crate::git::repo::base_of_range)
        .transpose()
        .map_err(|e| io::Error::other(format!("--pr-context: {e}")))?;
    let page = pr_context_page(run);
    run.report(|ctx, out| execute_pr_context_dispatch(&context_of(ctx), reference, page, out))
}

pub static PR_CONTEXT: Plugin<PrContextArgs> = Plugin {
    name: "pr-context",
    options: &[],
    selected: |args| args.pr_context.is_some(),
    run,
    check: None,
    tools: &[],
};
