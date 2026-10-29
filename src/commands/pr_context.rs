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

/// What `--pr-context VALUE` names.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PrContextTarget<'a> {
    /// A base revision, `REV` or `REV..HEAD`.
    Ref(&'a str),
    /// The repository path: `--pr-context .` with no ref.
    Path(&'a str),
}

/// A value that resolves as a revision stays a ref, so `--pr-context main` keeps its meaning; one
/// that does not but names a directory is the path; anything else is a ref and errors by name.
#[must_use]
pub fn pr_context_target(value: &str, is_revision: bool, is_dir: bool) -> PrContextTarget<'_> {
    if !is_revision && is_dir {
        PrContextTarget::Path(value)
    } else {
        PrContextTarget::Ref(value)
    }
}

fn run(run: &Run<'_>, args: &PrContextArgs) -> io::Result<()> {
    let spec = args.pr_context.clone().flatten();
    let target = spec.as_deref().map(|value| {
        let rev = crate::git::repo::base_of_range(value).unwrap_or(value);
        let is_revision = crate::git::repo::discover(run.root)
            .is_some_and(|repo| crate::git::repo::revision_commit(&repo, rev).is_some());
        pr_context_target(value, is_revision, std::path::Path::new(value).is_dir())
    });
    let (root, reference) = match target {
        Some(PrContextTarget::Path(path)) => (path, None),
        Some(PrContextTarget::Ref(value)) => (
            run.root,
            Some(
                crate::git::repo::base_of_range(value)
                    .map_err(|e| io::Error::other(format!("--pr-context: {e}")))?,
            ),
        ),
        None => (run.root, None),
    };
    let rooted = Run::new(run.cli, root, &[]);
    let page = pr_context_page(&rooted);
    rooted.report(|ctx, out| execute_pr_context_dispatch(&context_of(ctx), reference, page, out))
}

pub static PR_CONTEXT: Plugin<PrContextArgs> = Plugin {
    name: "pr-context",
    options: &[],
    selected: |args| args.pr_context.is_some(),
    run,
    check: None,
    tools: &[],
};
