//! Corpus search behind `--grep`, plus the modifiers it takes; `--match` reuses [`write_hits`] for its paged hit lists.
use crate::cli::TopModifierArgs;
use crate::commands::context::ExecutionContext;
use crate::serialize::PageRequest;
use serde_json::Value;
use std::io::{self, Write};

/// Searches the corpus for `pat` as a literal and writes the hits windowed by `page`.
pub fn execute<W: Write>(
    ctx: &ExecutionContext<'_>,
    pat: &str,
    page: PageRequest,
    in_scope: Option<&str>,
    modifiers: &TopModifierArgs,
    out: &mut W,
) -> io::Result<()> {
    let sources = crate::query::FileSources {
        root: std::path::Path::new(ctx.root),
    };
    let corpus = crate::crawler::walk_corpus(ctx.root, &crate::crawler::CrawlerOptions::default());
    let req = crate::query::retrieval::GrepRequest {
        corpus_paths: &corpus.rel_paths,
        pruned_dirs: corpus.pruned_dirs,
        limit: page.limit,
        offset: page.offset.unwrap_or(0),
        budget: page.budget,
        path_filter: in_scope,
        any_tier: crate::query::grep::parse_any_tier(modifiers.grep_in.as_deref())?,
        before: modifiers
            .grep_before
            .or(modifiers.grep_context)
            .unwrap_or(0),
        after: modifiers.grep_after.or(modifiers.grep_context).unwrap_or(0),
        and_terms: &modifiers.and_pat,
        not_terms: &modifiers.not_pat,
        file_scope: modifiers.grep_scope.as_deref() == Some("file"),
        graph: Some(ctx.graph),
        ..crate::query::retrieval::GrepRequest::new(ctx.root, pat, &sources)
    };
    let val = crate::query::retrieval::call_grep(ctx.ingest, ctx.interner, &req);
    crate::serialize::serialize_grep_xml(&val, out)
}

/// Writes grep-shaped hits, recording the `--limit` / `--offset` window so the report pages them.
pub fn write_hits<W: Write>(mut val: Value, page: PageRequest, out: &mut W) -> io::Result<()> {
    if let Some(limit) = page.limit {
        val["limit"] = Value::from(limit as u64);
    }
    if let Some(offset) = page.offset {
        val["offset"] = Value::from(offset as u64);
    }
    if let Some(budget) = page.budget {
        val["budget"] = Value::from(budget as u64);
    }
    crate::serialize::serialize_grep_xml(&val, out)
}
