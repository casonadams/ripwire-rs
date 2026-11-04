use crate::domain::test_path::is_test_path;
use crate::domain::{IngestResult, Interner, NodeId};
use crate::graph::Graph;
use crate::serialize::XmlEscapeExt;
use std::collections::{BTreeMap, BTreeSet};
use std::io::{self, Write};
use std::path::Path;

/// A single cross-module edge lacking test coverage.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeamEdge {
    pub caller: String,
    pub caller_path: String,
    pub callee: String,
    pub callee_path: String,
}

/// A cluster of untested call edges between two distinct directories.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeamGroup {
    pub from_dir: String,
    pub to_dir: String,
    pub edges: Vec<SeamEdge>,
}

/// Comprehensive report of cross-module integration seams.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SeamsReport {
    pub modules_count: usize,
    pub bridges_count: usize,
    pub untested_count: usize,
    pub test_files_count: usize,
    pub seam_pairs_count: usize,
    /// Cross-directory document links; `None` until a source reader counts them.
    pub doc_links: Option<usize>,
    pub groups: Vec<SeamGroup>,
    pub pr_iters: u32,
    pub gauges: crate::graph::GraphGauges,
    pub page: crate::serialize::PageRequest,
}

fn compute_directory(path: &str) -> String {
    Path::new(path)
        .parent()
        .and_then(|p| p.to_str())
        .filter(|s| !s.is_empty())
        .unwrap_or(".")
        .to_string()
}

fn check_cross_module_edge(
    sym: &crate::domain::Symbol,
    target_id: NodeId,
    ingest: &IngestResult,
    interner: &Interner,
    tested_mask: &[bool],
) -> Option<(String, String, bool, Option<SeamEdge>)> {
    let from_file = ingest.sym_file_path(sym, interner);
    if is_test_path(from_file) {
        return None;
    }
    let from_dir = compute_directory(from_file);

    let target_sym = ingest.sym(target_id)?;
    let to_file = ingest.sym_file_path(target_sym, interner);
    if is_test_path(to_file) {
        return None;
    }
    let to_dir = compute_directory(to_file);
    if from_dir == to_dir {
        return None;
    }

    let sym_tested = tested_mask.get(sym.id as usize).copied().unwrap_or(false);
    let tgt_tested = tested_mask
        .get(target_id as usize)
        .copied()
        .unwrap_or(false);
    let is_untested = !sym_tested || !tgt_tested;
    let seam_edge = if is_untested {
        Some(SeamEdge {
            caller: interner[sym.name].to_string(),
            caller_path: format!("{from_file}:{}", sym.line),
            callee: interner[target_sym.name].to_string(),
            callee_path: format!("{to_file}:{}", target_sym.line),
        })
    } else {
        None
    };

    Some((from_dir, to_dir, true, seam_edge))
}

fn collect_module_and_test_files(
    ingest: &IngestResult,
    interner: &Interner,
) -> (BTreeSet<String>, BTreeSet<String>) {
    let mut all_modules = BTreeSet::new();
    let mut test_files = BTreeSet::new();

    for f in &ingest.files {
        let p = interner.resolve(&f.path);
        all_modules.insert(compute_directory(p));
        if is_test_path(p) {
            test_files.insert(p.to_string());
        }
    }
    (all_modules, test_files)
}

fn process_sym_seams(
    sym: &crate::domain::Symbol,
    graph: &Graph,
    ingest: &IngestResult,
    interner: &Interner,
    tested_mask: &[bool],
    bridges_count: &mut usize,
    groups_map: &mut BTreeMap<(String, String), Vec<SeamEdge>>,
) {
    for (target_id, _) in graph.callees(sym.id) {
        if let Some((from_dir, to_dir, is_bridge, maybe_edge)) =
            check_cross_module_edge(sym, target_id, ingest, interner, tested_mask)
        {
            if is_bridge {
                *bridges_count += 1;
            }
            if let Some(edge) = maybe_edge {
                groups_map.entry((from_dir, to_dir)).or_default().push(edge);
            }
        }
    }
}

/// Detect untested cross-directory call edges across the workspace.
#[must_use]
pub fn find_integration_seams(
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    page: crate::serialize::PageRequest,
) -> SeamsReport {
    use crate::domain::CorpusView;
    let (all_modules, test_files) = collect_module_and_test_files(ingest, interner);
    let tested_mask = (ingest, interner).compute_test_reachability(graph);
    let mut bridges_count = 0usize;
    let mut groups_map: BTreeMap<(String, String), Vec<SeamEdge>> = BTreeMap::new();

    for sym in &ingest.symbols {
        process_sym_seams(
            sym,
            graph,
            ingest,
            interner,
            &tested_mask,
            &mut bridges_count,
            &mut groups_map,
        );
    }
    let mut groups = Vec::new();
    let mut untested_count = 0usize;

    for ((from_dir, to_dir), edges) in groups_map {
        untested_count += edges.len();
        groups.push(SeamGroup {
            from_dir,
            to_dir,
            edges,
        });
    }
    groups.sort_by(|a, b| {
        b.edges
            .len()
            .cmp(&a.edges.len())
            .then_with(|| a.from_dir.cmp(&b.from_dir))
            .then_with(|| a.to_dir.cmp(&b.to_dir))
    });
    SeamsReport {
        modules_count: all_modules.len(),
        bridges_count,
        untested_count,
        test_files_count: test_files.len(),
        seam_pairs_count: groups.len(),
        groups,
        doc_links: None,
        pr_iters: graph
            .page_rank(&crate::rank::PageRankConfig::default())
            .iterations
            .max(1),
        gauges: graph.gauges(),
        page,
    }
}

/// Resolve a relative link target against the directory of `doc`, normalising `.` and `..`.
fn resolve_doc_target(doc: &str, rel: &str) -> String {
    let mut parts: Vec<&str> = if rel.starts_with('/') {
        Vec::new()
    } else {
        doc.split('/').collect()
    };
    parts.pop();
    for seg in rel.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            s => parts.push(s),
        }
    }
    parts.join("/")
}

/// Markdown links from a prose document to another prose document in a different directory: the
/// `doc_links=` fact. Scheme links and anchors-only links do not count; the target must be indexed.
#[must_use]
pub fn count_doc_links(
    sources: &dyn crate::query::SourceReader,
    ingest: &IngestResult,
    interner: &Interner,
) -> usize {
    let prose: BTreeSet<&str> = ingest
        .files
        .iter()
        .filter(|f| f.lang.semantics().is_prose_document())
        .map(|f| interner.resolve(&f.path))
        .collect();
    let mut total = 0usize;
    for &doc in &prose {
        let Ok(bytes) = sources.read(doc) else {
            continue;
        };
        let Ok(text) = std::str::from_utf8(&bytes) else {
            continue;
        };
        let doc_dir = compute_directory(doc);
        total += crate::languages::markdown::extract::markdown_link_targets(text)
            .into_iter()
            .filter_map(|target| target.split('#').next())
            .filter(|file| !file.is_empty() && !file.contains(':'))
            .map(|file| resolve_doc_target(doc, file))
            .filter(|target| {
                prose.contains(target.as_str()) && compute_directory(target) != doc_dir
            })
            .count();
    }
    total
}

/// The edges of one seam group under `page`'s byte budget; `--limit`/`--offset` address groups, not edges.
fn seam_edge_window(
    group: &SeamGroup,
    page: crate::serialize::PageRequest,
) -> crate::serialize::PageWindow {
    crate::serialize::PageRequest {
        limit: None,
        offset: None,
        budget: page.budget,
    }
    .window_fit(group.edges.len(), |j| {
        crate::serialize::row_bytes(|w| write_seam_edge(&group.edges[j], w))
    })
}

fn write_seam_edge(e: &SeamEdge, w: &mut dyn Write) -> io::Result<()> {
    writeln!(
        w,
        "    <edge caller=\"{}\" p=\"{}\" callee=\"{}\" cp=\"{}\"/>",
        e.caller.xml_escaped(),
        e.caller_path.xml_escaped(),
        e.callee.xml_escaped(),
        e.callee_path.xml_escaped()
    )
}

fn write_seam_group(
    group: &SeamGroup,
    page: crate::serialize::PageRequest,
    w: &mut dyn Write,
) -> io::Result<()> {
    let edges = seam_edge_window(group, page);
    writeln!(
        w,
        "  <seam from=\"{}\" to=\"{}\" untested=\"{}\" shown=\"{}\" capped=\"{}\">",
        group.from_dir,
        group.to_dir,
        group.edges.len(),
        edges.shown,
        usize::from(edges.capped)
    )?;
    for e in &group.edges[edges.begin..edges.end] {
        write_seam_edge(e, w)?;
    }
    writeln!(w, "  </seam>")
}

impl SeamsReport {
    fn groups_paged(&self) -> crate::serialize::Paged<'_, SeamGroup> {
        crate::serialize::Paged::new(&self.groups, self.page, |g| {
            crate::serialize::row_bytes(|w| write_seam_group(g, self.page, w))
        })
    }
}

impl crate::serialize::XmlReport for SeamsReport {
    crate::xml_report_meta!(
        "seams",
        "ripwire-rs.seams/v1",
        root: true,
        comment: "ripwire-rs seams schema=ripwire-rs.seams/v1: cross-directory call edges NO test reaches."
    );
    fn page_window(&self) -> Option<crate::serialize::PageWindow> {
        Some(self.groups_paged().window)
    }
    fn pr_iters(&self) -> Option<u32> {
        Some(self.pr_iters)
    }
    fn graph_gauges(&self) -> Option<crate::graph::GraphGauges> {
        Some(self.gauges)
    }
    fn write_extra_attrs<W: Write>(&self, w: &mut W) -> io::Result<()> {
        let doc_links = self
            .doc_links
            .map(|n| format!(" doc_links=\"{n}\""))
            .unwrap_or_default();
        write!(
            w,
            " modules=\"{}\" bridges=\"{}\" untested=\"{}\"{doc_links} test_files=\"{}\" seam_pairs=\"{}\"",
            self.modules_count,
            self.bridges_count,
            self.untested_count,
            self.test_files_count,
            self.seam_pairs_count
        )
    }
    fn write_body<W: Write>(&self, w: &mut W) -> io::Result<()> {
        for group in self.groups_paged().rows() {
            write_seam_group(group, self.page, w)?;
        }
        Ok(())
    }
}
