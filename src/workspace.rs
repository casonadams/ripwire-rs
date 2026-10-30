use crate::domain::{IngestResult, Interner};
use crate::graph::Graph;
use crate::rank::PageRankResult;
use std::io;
use std::path::Path;

/// Indexed workspace facts shared by command transports.
pub struct Workspace {
    pub root: String,
    pub ingest: IngestResult,
    pub graph: Graph,
    pub interner: Interner,
    pub pr: PageRankResult,
    pub at: Option<String>,
    pub commits: Option<Vec<crate::git::history::CommitInfo>>,
    /// Working-tree stamp the index was built from; `None` for in-memory workspaces.
    stamp: Option<String>,
    scip_index: Option<std::path::PathBuf>,
    /// The workspace of the previous root, kept so alternating between two roots, or leaving a `--scip` launch
    /// root and coming back, does not re-index.
    parked: Option<Box<Workspace>>,
}

impl Workspace {
    fn load_with_cache(
        root: &str,
        no_cache: bool,
        scip_index: Option<&std::path::Path>,
    ) -> std::io::Result<Self> {
        let stamp = crate::cache::compute_cache_key(root);
        let (ingest, graph, pr, interner) = load_repo_analysis_with_options(
            root,
            None,
            no_cache,
            &crate::crawler::CrawlerOptions::default(),
            scip_index,
        )?;
        let mut state = Self::from_analysis(root, ingest, graph, pr, interner);
        state.at = crate::git::history::compute_git_at_stamp(root);
        state.stamp = Some(stamp);
        state.scip_index = scip_index.map(std::path::Path::to_path_buf);
        Ok(state)
    }

    /// Point `slot` at `target`, loading it when neither the active nor the parked workspace is for it. The
    /// displaced workspace is parked; of the two previously held, a SCIP-pinned one outlives a plain one. A failed
    /// load leaves `slot` untouched.
    pub fn select_root(slot: &mut Option<Self>, target: &str) -> Result<(), String> {
        Self::select_root_with(slot, target, Self::load)
    }

    /// [`Self::select_root`] with the loader for an unseen root supplied by the caller.
    pub fn select_root_with(
        slot: &mut Option<Self>,
        target: &str,
        load: impl FnOnce(&str) -> Option<Self>,
    ) -> Result<(), String> {
        if slot.as_ref().is_some_and(|ws| same_root(&ws.root, target)) {
            return Ok(());
        }
        let mut current = slot.take();
        let mut older = current.as_mut().and_then(|ws| ws.parked.take());
        let revived = older.as_ref().is_some_and(|ws| same_root(&ws.root, target));
        let next = if revived {
            older.take().map(|ws| *ws)
        } else {
            load(target)
        };
        let Some(mut next) = next else {
            *slot = current.map(|mut ws| {
                ws.parked = older;
                ws
            });
            return Err(format!("failed to index '{target}'"));
        };
        let keep_older = current.as_ref().is_some_and(|ws| !ws.has_scip())
            && older.as_ref().is_some_and(|ws| ws.has_scip());
        next.parked = if keep_older {
            older
        } else {
            current.map(Box::new)
        };
        *slot = Some(next);
        Ok(())
    }

    /// Whether a compiler-backed SCIP overlay pins this workspace's call sites.
    #[must_use]
    pub fn has_scip(&self) -> bool {
        self.scip_index.is_some()
    }

    /// Re-index when the working tree changed since this workspace was built; `Ok(true)` if it did.
    /// The stamp is taken before loading, so an edit racing the load triggers one more refresh.
    pub fn refresh_if_stale(&mut self) -> std::io::Result<bool> {
        let Some(stamp) = self.stamp.as_deref() else {
            return Ok(false);
        };
        if crate::cache::compute_cache_key(&self.root) == stamp {
            return Ok(false);
        }
        let mut fresh = Self::load_with_cache(&self.root, false, self.scip_index.as_deref())?;
        fresh.commits = self.commits.take();
        fresh.parked = self.parked.take();
        *self = fresh;
        Ok(true)
    }

    #[must_use]
    pub fn load(root: &str) -> Option<Self> {
        Self::load_with_cache(root, false, None).ok()
    }

    #[must_use]
    pub fn load_uncached(root: &str) -> Option<Self> {
        Self::load_with_cache(root, true, None).ok()
    }

    /// Load a workspace with an optional compiler index, preserving index errors.
    pub fn load_with_scip(
        root: &str,
        scip_index: Option<&std::path::Path>,
    ) -> std::io::Result<Self> {
        Self::load_with_cache(root, false, scip_index)
    }

    #[must_use]
    pub fn query_context<'a>(
        &'a self,
        sources: &'a dyn crate::query::SourceReader,
    ) -> crate::query::commands::QueryContext<'a> {
        crate::query::commands::QueryContext {
            root: &self.root,
            ingest: &self.ingest,
            graph: &self.graph,
            pr: &self.pr,
            interner: &self.interner,
            commits: self.commits.as_deref(),
            sources,
            at: self.at.as_deref(),
        }
    }

    #[must_use]
    pub fn safety_context(&self) -> crate::query::safety::SafetyContext<'_> {
        crate::query::safety::SafetyContext {
            root: &self.root,
            ingest: &self.ingest,
            graph: &self.graph,
            interner: &self.interner,
        }
    }
    #[must_use]
    pub fn from_ingest(root: &str, ingest: IngestResult, interner: Interner) -> Self {
        let (ingest, graph, pr, interner) =
            analyze_ingest(ingest, interner, None).expect("infallible without scip bytes");
        Self::from_analysis(root, ingest, graph, pr, interner)
    }

    pub(crate) fn from_analysis(
        root: &str,
        ingest: IngestResult,
        graph: Graph,
        pr: PageRankResult,
        interner: Interner,
    ) -> Self {
        Self {
            root: root.to_string(),
            ingest,
            graph,
            interner,
            pr,
            at: None,
            commits: None,
            stamp: None,
            scip_index: None,
            parked: None,
        }
    }

    #[must_use]
    pub fn with_commits(mut self, commits: Vec<crate::git::history::CommitInfo>) -> Self {
        self.commits = Some(commits);
        self
    }
}

fn same_root(a: &str, b: &str) -> bool {
    a == b
        || std::fs::canonicalize(a)
            .and_then(|a| std::fs::canonicalize(b).map(|b| a == b))
            .unwrap_or(false)
}

impl<'a> crate::domain::CorpusView<'a> for &'a Workspace {
    #[inline]
    fn ingest(&self) -> &'a IngestResult {
        &self.ingest
    }

    #[inline]
    fn interner(&self) -> &'a Interner {
        &self.interner
    }
}
pub fn load_repo_analysis(
    root: &str,
) -> io::Result<(crate::domain::IngestResult, Graph, PageRankResult, Interner)> {
    load_repo_analysis_cached(root, None, false)
}

pub fn load_repo_analysis_cached(
    root: &str,
    cache_path: Option<&Path>,
    no_cache: bool,
) -> io::Result<(crate::domain::IngestResult, Graph, PageRankResult, Interner)> {
    let opts = crate::crawler::CrawlerOptions::default();
    load_repo_analysis_with_options(root, cache_path, no_cache, &opts, None)
}

#[must_use]
pub fn is_analysis_cacheable(
    opts: &crate::crawler::CrawlerOptions,
    scip_index: Option<&Path>,
) -> bool {
    *opts == crate::crawler::CrawlerOptions::default() && scip_index.is_none()
}

pub fn analyze_ingest(
    mut ingest: crate::domain::IngestResult,
    interner: Interner,
    scip_bytes: Option<&[u8]>,
) -> io::Result<(crate::domain::IngestResult, Graph, PageRankResult, Interner)> {
    if let Some(bytes) = scip_bytes {
        ingest.scip_sites = crate::resolve::scip_sites(bytes, &ingest, &interner)
            .map_err(|e| io::Error::other(e.to_string()))?;
    }
    let graph = Graph::build(&ingest, &interner);
    let pr = graph.page_rank(&crate::rank::PageRankConfig::default());
    Ok((ingest, graph, pr, interner))
}

fn try_load_cached_analysis(
    root: &str,
    cache_path: Option<&Path>,
    cacheable: bool,
    no_cache: bool,
    options_tag: &str,
) -> Option<(crate::domain::IngestResult, Graph, PageRankResult, Interner)> {
    if cacheable && !no_cache {
        crate::cache::load_cached_analysis(root, cache_path, options_tag)
    } else {
        None
    }
}

fn read_scip_bytes(scip_index: Option<&Path>) -> io::Result<Option<Vec<u8>>> {
    let Some(path) = scip_index else {
        return Ok(None);
    };
    std::fs::read(path).map(Some).map_err(|e| {
        io::Error::new(
            e.kind(),
            format!("ripwire-rs: --scip {}: {e}", path.display()),
        )
    })
}

fn run_analysis(
    root: &str,
    opts: &crate::crawler::CrawlerOptions,
    scip_bytes: Option<&[u8]>,
) -> io::Result<(crate::domain::IngestResult, Graph, PageRankResult, Interner)> {
    let files = crate::crawler::crawl_directory(root, opts);
    let registry =
        crate::ingest::LanguageRegistry::new().map_err(|e| io::Error::other(e.to_string()))?;
    let interner = Interner::new();
    let ingest = crate::ingest::ingest_crawled_files(&files, &registry, &interner);
    analyze_ingest(ingest, interner, scip_bytes)
}

/// Crawl, ingest, and resolve `root`; a SCIP index pins the call sites it covers and bypasses
/// the cache, whose key covers crawler options but not the index.
pub fn load_repo_analysis_with_options(
    root: &str,
    cache_path: Option<&Path>,
    no_cache: bool,
    opts: &crate::crawler::CrawlerOptions,
    scip_index: Option<&Path>,
) -> io::Result<(crate::domain::IngestResult, Graph, PageRankResult, Interner)> {
    let cacheable = scip_index.is_none();
    let tag = opts.cache_tag();
    if let Some(cached) = try_load_cached_analysis(root, cache_path, cacheable, no_cache, &tag) {
        return Ok(cached);
    }
    let scip_bytes = read_scip_bytes(scip_index)?;
    run_analysis(root, opts, scip_bytes.as_deref()).inspect(|analysis| {
        if cacheable {
            let (ingest, graph, pr, interner) = analysis;
            let _ = crate::cache::save_cached_analysis(
                root,
                cache_path,
                &tag,
                (ingest, graph, pr, interner),
            );
        }
    })
}
