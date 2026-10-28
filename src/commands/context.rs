//! Shared plumbing for command plugins: output dialect, pagination, repository loading.
use crate::cli::{Cli, TopModifierArgs};
use crate::domain::Interner;
use crate::graph::Graph;
use crate::rank::PageRankResult;
use crate::workspace::load_repo_analysis_with_options;
use std::io::{self, Write};
use std::path::Path;

/// Capability trait for extracting pagination windows (`--limit`, `--offset`) and the output budget.
pub trait Paginated {
    fn limit_opt(&self) -> Option<usize>;
    fn offset_opt(&self) -> Option<usize>;

    /// The byte budget default row caps are computed from (`--token-budget`, `--pack-budget`, `--max-tokens`).
    fn budget_opt(&self) -> Option<usize> {
        None
    }

    fn page_request(&self) -> crate::serialize::PageRequest {
        crate::serialize::PageRequest {
            limit: self.limit_opt(),
            offset: self.offset_opt(),
            budget: self.budget_opt(),
        }
    }

    fn apply_pagination_to_json(&self, val: &mut serde_json::Value) {
        if let Some(lim) = self.limit_opt() {
            val["limit"] = serde_json::Value::from(lim as u64);
        }
        if let Some(off) = self.offset_opt() {
            val["offset"] = serde_json::Value::from(off as u64);
        }
        if let Some(budget) = self.budget_opt() {
            val["budget"] = serde_json::Value::from(budget as u64);
        }
    }
}

/// Capability trait for output dialect selection (`--json`, `--pretty`, `--legend`).
pub trait OutputDialect {
    fn is_json_out(&self) -> bool;
    fn is_pretty_out(&self) -> bool;
    fn is_legend_none(&self) -> bool;

    fn wrap_xml_writer<'a, W: std::io::Write>(
        &self,
        inner: &'a mut W,
        force_plain: bool,
    ) -> crate::serialize::XmlWriter<'a, W> {
        crate::serialize::XmlWriter::with_options(
            inner,
            self.is_pretty_out() || force_plain,
            self.is_json_out() && !force_plain,
            self.is_legend_none() && !force_plain,
        )
    }
}
impl Paginated for TopModifierArgs {
    fn limit_opt(&self) -> Option<usize> {
        self.limit
    }
    fn offset_opt(&self) -> Option<usize> {
        self.offset
    }
}

impl Paginated for Cli {
    fn limit_opt(&self) -> Option<usize> {
        self.modifiers.limit
    }
    fn offset_opt(&self) -> Option<usize> {
        self.modifiers.offset
    }
    fn budget_opt(&self) -> Option<usize> {
        self.effective_budget_bytes()
    }
}

#[must_use]
pub fn crawler_options_for_cli(cli: &Cli) -> crate::crawler::CrawlerOptions {
    let defaults = crate::crawler::CrawlerOptions::default();
    crate::crawler::CrawlerOptions {
        max_file_size: cli
            .modifiers
            .max_file_size
            .unwrap_or(defaults.max_file_size),
        honor_gitignore: !cli.modifiers.no_ignore,
        ignore_tests: cli.modifiers.ignore_tests,
        custom_excludes: cli
            .modifiers
            .exclude
            .iter()
            .flat_map(|s| split_csv_patterns(s))
            .map(String::from)
            .collect(),
    }
}

pub fn load_repo_analysis_for_cli(
    cli: &Cli,
    root: &str,
    force_uncached: bool,
) -> io::Result<(crate::domain::IngestResult, Graph, PageRankResult, Interner)> {
    let cache_path = cli.modifiers.cache.as_deref().map(Path::new);
    let opts = crawler_options_for_cli(cli);
    load_repo_analysis_with_options(
        root,
        cache_path,
        cli.modifiers.no_cache || force_uncached,
        &opts,
        cli.scip.as_deref().map(Path::new),
    )
}

pub fn split_csv_patterns(raw: &str) -> Vec<&str> {
    raw.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect()
}

/// Shared execution context bundling CLI options, repository root, and indexed analysis state.
pub struct ExecutionContext<'a> {
    pub cli: &'a Cli,
    pub root: &'a str,
    pub ingest: &'a crate::domain::IngestResult,
    pub graph: &'a Graph,
    pub pr: &'a PageRankResult,
    pub interner: &'a Interner,
}

impl<'a> crate::domain::CorpusView<'a> for ExecutionContext<'a> {
    #[inline]
    fn ingest(&self) -> &'a crate::domain::IngestResult {
        self.ingest
    }

    #[inline]
    fn interner(&self) -> &'a Interner {
        self.interner
    }
}

impl Paginated for ExecutionContext<'_> {
    #[inline]
    fn limit_opt(&self) -> Option<usize> {
        self.cli.limit_opt()
    }

    #[inline]
    fn offset_opt(&self) -> Option<usize> {
        self.cli.offset_opt()
    }

    #[inline]
    fn budget_opt(&self) -> Option<usize> {
        self.cli.budget_opt()
    }
}

impl ExecutionContext<'_> {
    #[inline]
    pub fn write_report<R: crate::serialize::XmlReport, W: Write>(
        &self,
        rep: &R,
        out: &mut W,
    ) -> io::Result<bool> {
        rep.write_xml_report(self.root, out)?;
        Ok(true)
    }
}

impl OutputDialect for Cli {
    fn is_json_out(&self) -> bool {
        self.modifiers.is_json()
    }
    fn is_pretty_out(&self) -> bool {
        self.modifiers.is_pretty()
    }
    fn is_legend_none(&self) -> bool {
        self.legend.eq_ignore_ascii_case("none")
    }
}
