use crate::domain::{CorpusView, IngestResult, Interner, Symbol};
use crate::graph::Graph;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::ops::Range;

/// Caller-file and test rows listed per changed file; `files_other=` and `<tests count=>` still
/// count every one.
pub const MAX_DETAIL_ROWS: usize = 5;

/// Tokens of report header and legend charged on top of the rows in `est_tokens=`.
const HEADER_TOKENS: usize = 180;

/// Lines a change touched on the new side of one file, and the byte offset each line starts at.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChangedLines {
    /// 0-based, half-open line ranges; an empty range marks lines deleted before that line.
    pub ranges: Vec<Range<u32>>,
    pub line_starts: Vec<u32>,
}

/// A file outside the changed one that calls its changed symbols, with the call edges it holds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallerFile {
    pub path: String,
    pub deps: usize,
}

/// PR Review context for a single modified file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrFileContext {
    pub path: String,
    /// Symbols defined in the file.
    pub symbol_count: usize,
    /// Symbols that own a changed line; every symbol of the file when no line diff is known.
    pub changed_symbols: Vec<String>,
    pub dependents_count: usize,
    /// Caller files, most call edges first.
    pub callers: Vec<CallerFile>,
    /// Tests that reach the changed symbols; `run` is filled by the caller, which can read sources.
    pub tests: Vec<crate::gate::affected::AffectedTest>,
    pub history: Option<FileHistory>,
}

/// Git history of one changed file. Absent when the file has no commits, so no cochange or
/// owner claim is made for it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileHistory {
    pub commits: usize,
    pub partners: usize,
    pub authors: usize,
    pub bus_factor: usize,
}

/// Comprehensive PR context report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PrContextReport {
    pub base_ref: Option<String>,
    pub at_stamp: String,
    pub files_count: usize,
    pub files: Vec<PrFileContext>,
    pub gauges: crate::serialize::GraphGauges,
    pub page: crate::serialize::PageRequest,
}

#[derive(Debug, Clone, Copy)]
pub struct PrContextRequest<'a> {
    pub changed_files: &'a [String],
    /// New-side changed lines per path; a path without an entry is attributed whole-file.
    pub changed_lines: Option<&'a BTreeMap<String, ChangedLines>>,
    pub base_ref: Option<&'a str>,
    pub at_stamp: &'a str,
    pub commits: &'a [crate::git::history::CommitInfo],
    pub page: crate::serialize::PageRequest,
}

struct Corpus<'a> {
    ingest: &'a IngestResult,
    graph: &'a Graph,
    interner: &'a Interner,
    coupling: &'a crate::git::cochange::StaticCouplingMap,
    commits: &'a [crate::git::history::CommitInfo],
}

fn line_of(starts: &[u32], byte: u32) -> usize {
    starts.partition_point(|&s| s <= byte).saturating_sub(1)
}

/// The index into `syms` of the innermost symbol spanning each line, outermost painted first.
fn paint_owners(syms: &[&Symbol], starts: &[u32]) -> Vec<Option<usize>> {
    let lines = starts.len();
    let mut spans: Vec<(usize, usize, usize)> = syms
        .iter()
        .enumerate()
        .map(|(i, s)| {
            let first = line_of(starts, s.sig_start_byte);
            let last = line_of(starts, s.end_byte.max(s.sig_start_byte + 1) - 1);
            (first, last.max(first), i)
        })
        .filter(|&(first, _, _)| first < lines)
        .collect();
    spans.sort_by_key(|&(first, last, i)| (std::cmp::Reverse(last - first), i));
    let mut owner = vec![None; lines];
    for (first, last, i) in spans {
        for slot in &mut owner[first..=last.min(lines - 1)] {
            *slot = Some(i);
        }
    }
    owner
}

/// Owners of one changed range. A pure deletion counts only for a symbol that holds the lines on
/// both sides of it, so deleting a whole definition marks neither neighbour.
fn range_owners(owner: &[Option<usize>], range: &Range<u32>) -> Vec<usize> {
    let end = (range.end as usize).min(owner.len());
    let start = (range.start as usize).min(end);
    if start < end {
        return owner[start..end].iter().flatten().copied().collect();
    }
    let before = start
        .checked_sub(1)
        .and_then(|k| owner.get(k).copied().flatten());
    let after = owner.get(start).copied().flatten();
    match (before, after) {
        (Some(a), Some(b)) if a == b => vec![a],
        _ => Vec::new(),
    }
}

/// The symbols of one file that own a changed line: each line belongs to the innermost definition
/// spanning it, so editing a method marks the method and not its enclosing type.
#[must_use]
pub fn touched_symbols<'s>(syms: &[&'s Symbol], changed: &ChangedLines) -> Vec<&'s Symbol> {
    let owner = paint_owners(syms, &changed.line_starts);
    let hit: BTreeSet<usize> = changed
        .ranges
        .iter()
        .flat_map(|range| range_owners(&owner, range))
        .collect();
    hit.into_iter().map(|i| syms[i]).collect()
}

fn caller_files(syms: &[&Symbol], corpus: &Corpus<'_>) -> Vec<CallerFile> {
    let view = (corpus.ingest, corpus.interner);
    let mut deps: BTreeMap<&str, usize> = BTreeMap::new();
    for sym in syms {
        for (caller_id, _) in corpus.graph.callers(sym.id) {
            if let Some(cs) = corpus.ingest.sym(caller_id)
                && cs.file_id != sym.file_id
            {
                *deps.entry(view.sym_path(cs)).or_default() += 1;
            }
        }
    }
    let mut rows: Vec<CallerFile> = deps
        .into_iter()
        .map(|(path, deps)| CallerFile {
            path: path.to_string(),
            deps,
        })
        .collect();
    rows.sort_by(|a, b| b.deps.cmp(&a.deps).then_with(|| a.path.cmp(&b.path)));
    rows
}

fn affected_tests(
    f: &str,
    touched: Option<&[&Symbol]>,
    corpus: &Corpus<'_>,
) -> Vec<crate::gate::affected::AffectedTest> {
    let report = match touched {
        Some(syms) => {
            let ids: Vec<_> = syms.iter().map(|s| s.id).collect();
            crate::gate::affected::find_affected_tests_for_symbols(
                &ids,
                &[f],
                corpus.ingest,
                corpus.graph,
                corpus.interner,
            )
        }
        None => crate::gate::affected::find_affected_tests(
            &[f],
            corpus.ingest,
            corpus.graph,
            corpus.interner,
        ),
    };
    report.tests
}

/// Commits, authors and the `--owners` single-owner flag of one file, from recency-weighted commits.
fn file_history(f: &str, corpus: &Corpus<'_>) -> Option<FileHistory> {
    use crate::query::branch_tools::{recency_weight, single_owner_flag};
    let head_epoch = corpus.commits.first().map_or(0, |c| c.timestamp);
    let mut file_commits = 0usize;
    let mut weights: BTreeMap<&str, f64> = BTreeMap::new();
    for c in corpus.commits {
        if c.changed_files.iter().any(|cf| cf == f || cf.ends_with(f)) {
            file_commits += 1;
            *weights.entry(c.author.as_str()).or_default() +=
                recency_weight(head_epoch, c.timestamp);
        }
    }
    let total: f64 = weights.values().sum();
    let top = weights.values().copied().fold(0.0, f64::max);
    let share = if total > 0.0 { top / total } else { 1.0 };
    (file_commits > 0).then(|| FileHistory {
        commits: file_commits,
        partners: corpus.coupling.partners_of(f).len(),
        authors: weights.len(),
        bus_factor: single_owner_flag(share),
    })
}

fn build_single_pr_file_context(
    f: &str,
    lines: Option<&ChangedLines>,
    corpus: &Corpus<'_>,
) -> PrFileContext {
    let view = (corpus.ingest, corpus.interner);
    let in_file: Vec<&Symbol> = corpus
        .ingest
        .symbols
        .iter()
        .filter(|sym| {
            let path = view.sym_path(sym);
            path == f || path.ends_with(f)
        })
        .collect();
    let touched = lines.map(|lines| touched_symbols(&in_file, lines));
    let changed = touched.as_deref().unwrap_or(&in_file);
    PrFileContext {
        path: f.to_string(),
        symbol_count: in_file.len(),
        changed_symbols: changed
            .iter()
            .map(|s| view.sym_name(s).to_string())
            .collect(),
        dependents_count: changed.iter().map(|s| corpus.graph.in_degree(s.id)).sum(),
        callers: caller_files(changed, corpus),
        tests: affected_tests(f, touched.as_deref(), corpus),
        history: file_history(f, corpus),
    }
}

#[must_use]
pub fn compute_pr_context_from_facts(
    req: &PrContextRequest<'_>,
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
) -> PrContextReport {
    let coupling = crate::git::cochange::StaticCouplingMap::build(ingest, interner, Some(graph));
    let corpus = Corpus {
        ingest,
        graph,
        interner,
        coupling: &coupling,
        commits: req.commits,
    };
    let files: Vec<PrFileContext> = req
        .changed_files
        .iter()
        .map(|f| {
            let lines = req.changed_lines.and_then(|m| m.get(f));
            build_single_pr_file_context(f, lines, &corpus)
        })
        .collect();

    PrContextReport {
        base_ref: req.base_ref.map(str::to_string),
        at_stamp: req.at_stamp.to_string(),
        files_count: files.len(),
        files,
        gauges: graph.gauges(),
        page: req.page,
    }
}

/// ` shown="N" capped="1"` when `total` rows exceed [`MAX_DETAIL_ROWS`]; empty otherwise.
fn detail_window(total: usize) -> String {
    if total > MAX_DETAIL_ROWS {
        format!(" shown=\"{MAX_DETAIL_ROWS}\" capped=\"1\"")
    } else {
        String::new()
    }
}

fn write_impact(f: &PrFileContext, out: &mut dyn Write) -> io::Result<()> {
    let head = format!(
        "    <impact dependents=\"{}\" files_other=\"{}\"{}",
        f.dependents_count,
        f.callers.len(),
        detail_window(f.callers.len())
    );
    if f.callers.is_empty() {
        return writeln!(out, "{head}/>");
    }
    writeln!(out, "{head}>")?;
    for c in f.callers.iter().take(MAX_DETAIL_ROWS) {
        let deps = if c.deps > 1 {
            format!(" deps=\"{}\"", c.deps)
        } else {
            String::new()
        };
        writeln!(
            out,
            "      <f p=\"{}\"{deps}/>",
            crate::serialize::XmlEscaped(&c.path)
        )?;
    }
    writeln!(out, "    </impact>")
}

fn write_tests(f: &PrFileContext, out: &mut dyn Write) -> io::Result<()> {
    let head = format!(
        "    <tests count=\"{}\"{}",
        f.tests.len(),
        detail_window(f.tests.len())
    );
    if f.tests.is_empty() {
        return writeln!(out, "{head}/>");
    }
    writeln!(out, "{head}>")?;
    for t in f.tests.iter().take(MAX_DETAIL_ROWS) {
        writeln!(
            out,
            "      <test p=\"{}\"{}/>",
            crate::serialize::XmlEscaped(&t.path),
            t.run_attr()
        )?;
    }
    writeln!(out, "    </tests>")
}

/// Serialize PR context to canonical XML format.
fn write_pr_file_context(f: &PrFileContext, out: &mut dyn Write) -> io::Result<()> {
    writeln!(
        out,
        "  <file p=\"{}\" symbols=\"{}\">",
        f.path, f.symbol_count
    )?;
    writeln!(
        out,
        "    <changed-symbols count=\"{}\">",
        f.changed_symbols.len()
    )?;
    for s in &f.changed_symbols {
        writeln!(out, "      <s n=\"{}\"/>", crate::serialize::XmlEscaped(s))?;
    }
    writeln!(out, "    </changed-symbols>")?;
    write_impact(f, out)?;
    write_tests(f, out)?;
    if let Some(h) = &f.history {
        writeln!(
            out,
            "    <cochange commits=\"{}\" partners=\"{}\"/>",
            h.commits, h.partners
        )?;
        writeln!(
            out,
            "    <owners authors=\"{}\" bf=\"{}\"/>",
            h.authors, h.bus_factor
        )?;
    }
    writeln!(out, "  </file>")
}

fn file_row_bytes(f: &PrFileContext) -> usize {
    crate::serialize::row_bytes(|w| write_pr_file_context(f, w))
}

impl PrContextReport {
    fn files_paged(&self) -> crate::serialize::Paged<'_, PrFileContext> {
        crate::serialize::Paged::new(&self.files, self.page, file_row_bytes)
    }
}

impl crate::serialize::XmlReport for PrContextReport {
    crate::xml_report_meta!(
        "pr-context",
        "ripwire-rs.pr-context/v1",
        root: true,
        comment: "ripwire-rs pr-context schema=ripwire-rs.pr-context/v1: review bundle per changed file vs base=: <s> changed symbols (owners of changed lines), <impact> caller files <f p= deps=>, <tests> <test p= run=|run_unknown=>; at most 5 rows each, counts cover all."
    );
    fn page_window(&self) -> Option<crate::serialize::PageWindow> {
        Some(self.files_paged().window)
    }
    fn graph_gauges(&self) -> Option<crate::serialize::GraphGauges> {
        Some(self.gauges)
    }
    fn write_extra_attrs<W: Write>(&self, w: &mut W) -> io::Result<()> {
        let paged = self.files_paged();
        let window = paged.window;
        let shown_bytes: usize = paged.rows().iter().map(file_row_bytes).sum();
        let est_tokens = HEADER_TOKENS + shown_bytes.div_ceil(crate::serialize::BYTES_PER_TOKEN);
        let budget = self
            .page
            .budget
            .unwrap_or(crate::serialize::DEFAULT_PACK_BUDGET_BYTES);
        let budget_tokens = budget / crate::serialize::BYTES_PER_TOKEN;
        let trim_level = usize::from(window.capped);
        let truncated = if window.capped { "files" } else { "none" };
        let budget_default = usize::from(self.page.budget.is_none());
        let base = self.base_ref.as_deref().unwrap_or("working-tree");
        let direction = if self.base_ref.is_some() {
            "worktree-since-merge-base"
        } else {
            "worktree-since-head"
        };
        let at_attr = if self.at_stamp.is_empty() {
            String::new()
        } else {
            format!(" at=\"{}\"", crate::serialize::XmlEscaped(&self.at_stamp))
        };
        write!(
            w,
            " base=\"{}\" direction=\"{direction}\" files=\"{}\" budget_tokens=\"{budget_tokens}\" est_tokens=\"{est_tokens}\" trim_level=\"{trim_level}\" truncated=\"{truncated}\" budget_default=\"{budget_default}\"{at_attr}",
            crate::serialize::XmlEscaped(base),
            self.files_count
        )
    }
    fn write_body<W: Write>(&self, w: &mut W) -> io::Result<()> {
        for f in self.files_paged().rows() {
            write_pr_file_context(f, w)?;
        }
        Ok(())
    }
}

/// Serialize PR context to canonical XML format.
pub fn serialize_pr_context_xml<W: Write>(rep: &PrContextReport, out: &mut W) -> io::Result<()> {
    use crate::serialize::ToXmlString;
    rep.write_xml(out)
}
