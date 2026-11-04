//! Plumbing shared by the retrieval commands: the [`QueryContext`] they run against and the token budget.
use crate::cli::Cli;
use crate::commands::Run;
use crate::query::commands::QueryContext;
use crate::serialize::XmlWriter;
use std::io::{self, StdoutLock};
use std::path::Path;

/// Loads the analysis and runs `body` over it as a [`QueryContext`], stamped with the repository's git state.
pub fn report_query(
    run: &Run<'_>,
    body: impl FnOnce(&QueryContext<'_>, &mut XmlWriter<'_, StdoutLock<'static>>) -> io::Result<()>,
) -> io::Result<()> {
    run.report(|ctx, out| {
        let sources = crate::query::FileSources {
            root: Path::new(ctx.root),
        };
        let at = crate::git::history::compute_git_at_stamp(ctx.root).map_err(io::Error::other)?;
        let query = QueryContext {
            root: ctx.root,
            ingest: ctx.ingest,
            graph: ctx.graph,
            pr: ctx.pr,
            interner: ctx.interner,
            sources: &sources,
            commits: None,
            at: at.as_deref(),
        };
        body(&query, out)
    })
}

/// The `--token-budget` / `--max-tokens` ceiling for task bundles.
pub fn token_budget(cli: &Cli) -> Option<usize> {
    cli.token_budget.or(cli.max_tokens)
}
