use crate::domain::test_path::is_test_path;
use crate::domain::{IngestResult, Interner, Lang, NodeId, RefRole};
use crate::graph::Graph;
use crate::graph::value_reach::ValueReach;
use crate::languages::has_dir_component;
use crate::serialize::XmlEscapeExt;
use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{self, Write};
use std::path::Path;

/// A test file affected by changes, with its hop distance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffectedTest {
    pub path: String,
    pub hops: u32,
    pub is_direct_change: bool,
    /// Named after a changed file by test convention; `hops` is 0 unless the graph also reached it.
    pub is_partner: bool,
    /// Shell command that runs this test, when its language's runner is derivable.
    pub run: Option<String>,
    /// A registered shell gate that names a changed file by exact path.
    pub script_literal: bool,
}

impl AffectedTest {
    #[must_use]
    pub fn new(path: String, hops: u32, is_direct_change: bool, is_partner: bool) -> Self {
        Self {
            path,
            hops,
            is_direct_change,
            is_partner,
            run: None,
            script_literal: false,
        }
    }

    /// ` run="CMD"` when a runner is derivable, else ` run_unknown="1"`.
    #[must_use]
    pub fn run_attr(&self) -> String {
        match &self.run {
            Some(cmd) => format!(" run=\"{}\"", cmd.xml_escaped()),
            None => " run_unknown=\"1\"".to_string(),
        }
    }
}

/// Comprehensive affected test reachability report.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AffectedReport {
    pub changed_spec: String,
    /// The reading that seeded the walk (`file`, `symbol`, `mixed`, `none`).
    pub seeded_by: &'static str,
    pub seed_count: usize,
    pub seed_test_files: Vec<String>,
    pub total_tests: usize,
    pub reached_symbols: usize,
    /// Test runners the call graph does not model (`test/*.sh`); set by the caller that knows the root.
    pub script_gates_unmodelled: usize,
    /// Symbols that transitively call the change, seeds excluded, ascending.
    pub impacted: Vec<NodeId>,
    pub tests: Vec<AffectedTest>,
}

impl crate::serialize::XmlReport for AffectedReport {
    crate::xml_report_meta!("affected", "ripwire-rs.affected/v1", root: false);
    fn write_extra_attrs<W: Write>(&self, out: &mut W) -> io::Result<()> {
        let partners = self.tests.iter().filter(|t| t.is_partner).count();
        write!(
            out,
            " changed=\"{}\" seeded_by=\"{}\" seeds=\"{}\" seed_test_files=\"{}\" tests=\"{}\" reached=\"{}\" script_gates_unmodelled=\"{}\" order=\"evidence\" partners=\"{partners}\"",
            self.changed_spec.xml_escaped(),
            self.seeded_by,
            self.seed_count,
            self.seed_test_files.len(),
            self.total_tests,
            self.reached_symbols,
            self.script_gates_unmodelled
        )
    }
    fn write_body<W: Write>(&self, out: &mut W) -> io::Result<()> {
        crate::serialize::arch_xml::write_test_rows(
            &self.tests,
            affected_flags,
            AffectedTest::write_row,
            out,
        )
    }
}

impl AffectedTest {
    fn write_row<W: Write>(&self, out: &mut W) -> io::Result<()> {
        write!(
            out,
            "<test p=\"{}\"{}{}/>",
            self.path.xml_escaped(),
            affected_flags(self),
            self.run_attr()
        )
    }
}

fn affected_flags(t: &AffectedTest) -> String {
    let partner = if t.is_partner { " partner=\"1\"" } else { "" };
    let hops = if !t.is_partner || t.hops > 0 {
        format!(" hops=\"{}\"", t.hops)
    } else {
        String::new()
    };
    let seed_kind = if t.is_direct_change {
        " seed_kind=\"test\""
    } else {
        ""
    };
    format!("{partner}{hops}{seed_kind}")
}

/// The source stem a test file at `path` is named after, by its language's test-naming convention
/// (the universal `test_x`, `x_test`, `x.spec` rules, plus `XTest` inside a `test/` or `tests/` dir).
#[must_use]
pub fn partner_subject_stem(path: &str, lang: Lang) -> Option<&str> {
    let sem = lang.semantics();
    if !sem.supports_test_partners() {
        return None;
    }
    let name = path.rsplit('/').next().unwrap_or(path);
    let (stem, _) = name.rsplit_once('.')?;
    let in_test_dir = has_dir_component(Path::new(path), |d| matches!(d, "test" | "tests"));
    sem.test_partner_subject(stem, in_test_dir)
}

fn file_stem_of(path: &str) -> &str {
    let name = path.rsplit('/').next().unwrap_or(path);
    name.rsplit_once('.').map_or(name, |(stem, _)| stem)
}

/// Test files named after any of `changed_paths` by convention, sorted by path. Every candidate
/// counts, including a changed test and tests that share a subject across directories.
#[must_use]
pub fn find_partner_tests(
    changed_paths: &[String],
    ingest: &IngestResult,
    interner: &Interner,
) -> Vec<String> {
    let stems: HashSet<&str> = changed_paths.iter().map(|p| file_stem_of(p)).collect();
    let mut partners: Vec<String> = ingest
        .files
        .iter()
        .filter_map(|f| {
            let p = interner.resolve(&f.path);
            partner_subject_stem(p, f.lang)
                .filter(|s| stems.contains(s))
                .map(|_| p.to_string())
        })
        .collect();
    partners.sort_unstable();
    partners
}

/// Assertion scripts that import a changed source file: static import reach, for scripts whose imports
/// the call graph does not follow (type-only imports). Only files a language names as assertion scripts.
fn assertion_importers(
    seed_source_files: &[String],
    ingest: &IngestResult,
    interner: &Interner,
) -> Vec<String> {
    let file_index: HashMap<&str, u32> = ingest
        .files
        .iter()
        .map(|f| (interner.resolve(&f.path), f.id))
        .collect();
    let mut out: Vec<String> = ingest
        .references
        .iter()
        .filter(|r| r.role == RefRole::Import)
        .filter_map(|r| {
            let file = ingest.files.get(r.file_id as usize)?;
            let path = interner.resolve(&file.path);
            if !file.lang.semantics().is_assertion_script(path) {
                return None;
            }
            let raw = interner.resolve(&r.callee_name);
            let target = crate::languages::resolve_import(
                file.lang,
                path,
                raw,
                &file_index,
                &ingest.import_aliases,
            )?;
            let target_path = interner.resolve(&ingest.files.get(target as usize)?.path);
            seed_source_files
                .iter()
                .any(|s| s == target_path)
                .then(|| path.to_string())
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

struct SeedResolution {
    seed_nodes: Vec<NodeId>,
    seed_test_files: Vec<String>,
    seed_source_files: Vec<String>,
    /// A changed spec named an indexed file by path.
    by_file: bool,
    /// A spec named no file and matched symbols by name.
    by_symbol: bool,
}

impl SeedResolution {
    /// The reading that seeded the walk: `file`, `symbol`, `mixed`, or `none`.
    fn seeded_by(&self) -> &'static str {
        match (self.by_file, self.by_symbol) {
            (true, true) => "mixed",
            (true, false) => "file",
            (false, true) => "symbol",
            (false, false) => "none",
        }
    }
}

fn push_unique(list: &mut Vec<String>, path: &str) {
    if !list.iter().any(|p| p == path) {
        list.push(path.to_string());
    }
}

fn classify_file_seed(
    f: &crate::domain::FileInfo,
    path_str: &str,
    ingest: &IngestResult,
    seeds: &mut SeedResolution,
) {
    if is_test_path(path_str) {
        if ingest.symbols.iter().any(|s| s.file_id == f.id) {
            push_unique(&mut seeds.seed_test_files, path_str);
        }
    } else {
        // A changed source file names partner tests even with no symbols.
        push_unique(&mut seeds.seed_source_files, path_str);
    }
}

fn match_file_seeds(
    f: &crate::domain::FileInfo,
    path_str: &str,
    ingest: &IngestResult,
    seeds: &mut SeedResolution,
) {
    classify_file_seed(f, path_str, ingest, seeds);
    if !is_test_path(path_str) {
        seeds.seed_nodes.extend(
            ingest
                .symbols
                .iter()
                .filter(|s| s.file_id == f.id)
                .map(|s| s.id),
        );
    }
}

fn match_symbol_seeds(
    spec: &str,
    ingest: &IngestResult,
    interner: &Interner,
    seeds: &mut SeedResolution,
) {
    for s in &ingest.symbols {
        if interner.resolve(&s.name) == spec {
            seeds.seed_nodes.push(s.id);
            seeds.by_symbol = true;
        }
    }
}

fn resolve_spec_seeds(
    spec: &str,
    ingest: &IngestResult,
    interner: &Interner,
    seeds: &mut SeedResolution,
) {
    let mut matched_file = false;
    for f in &ingest.files {
        let path_str = interner.resolve(&f.path);
        if path_str == spec || path_str.ends_with(spec) {
            matched_file = true;
            seeds.by_file = true;
            match_file_seeds(f, path_str, ingest, seeds);
        }
    }

    if !matched_file {
        match_symbol_seeds(spec, ingest, interner, seeds);
    }
}

fn traverse_reverse_callers(
    seed_nodes: &[NodeId],
    graph: &Graph,
    value: &ValueReach,
    ingest: &IngestResult,
    interner: &Interner,
) -> (HashSet<NodeId>, HashMap<String, u32>) {
    let mut visited: HashSet<NodeId> = HashSet::new();
    let mut queue: VecDeque<(NodeId, u32)> = VecDeque::new();
    let mut test_hops: HashMap<String, u32> = HashMap::new();

    for &node in seed_nodes {
        visited.insert(node);
        queue.push_back((node, 0));
    }

    while let Some((curr, hops)) = queue.pop_front() {
        if let Some(sym) = ingest.sym(curr) {
            let file_path = ingest.sym_file_path(sym, interner).to_string();
            if is_test_path(&file_path) && hops > 0 {
                test_hops
                    .entry(file_path)
                    .and_modify(|h| *h = (*h).min(hops))
                    .or_insert(hops);
            }
        }

        for caller in graph
            .callers(curr)
            .map(|(c, _)| c)
            .chain(value.readers(curr))
        {
            if visited.insert(caller) {
                queue.push_back((caller, hops + 1));
            }
        }
    }

    (visited, test_hops)
}

/// Find test files that transitively reach the changed files or symbols.
#[must_use]
pub fn find_affected_tests(
    changed_specs: &[&str],
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
) -> AffectedReport {
    let mut seeds = SeedResolution {
        seed_nodes: Vec::new(),
        seed_test_files: Vec::new(),
        seed_source_files: Vec::new(),
        by_file: false,
        by_symbol: false,
    };
    for &spec in changed_specs {
        resolve_spec_seeds(spec.trim(), ingest, interner, &mut seeds);
    }
    affected_from_seeds(seeds, changed_specs.join(","), graph, ingest, interner)
}

/// Find test files that reach the given symbols by reverse calls. Changed paths only name partner
/// and direct tests; they do not widen the seed set to every symbol in those files.
#[must_use]
pub fn find_affected_tests_for_symbols(
    symbols: &[NodeId],
    changed_paths: &[&str],
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
) -> AffectedReport {
    let mut seeds = SeedResolution {
        seed_nodes: symbols.to_vec(),
        seed_test_files: Vec::new(),
        seed_source_files: Vec::new(),
        by_file: false,
        by_symbol: !symbols.is_empty(),
    };
    for f in &ingest.files {
        let path_str = interner.resolve(&f.path);
        if changed_paths.contains(&path_str) {
            seeds.by_file = true;
            classify_file_seed(f, path_str, ingest, &mut seeds);
        }
    }
    affected_from_seeds(seeds, changed_paths.join(","), graph, ingest, interner)
}

fn affected_from_seeds(
    mut seeds: SeedResolution,
    changed_spec: String,
    graph: &Graph,
    ingest: &IngestResult,
    interner: &Interner,
) -> AffectedReport {
    seeds.seed_nodes.sort_unstable();
    seeds.seed_nodes.dedup();

    let value = ValueReach::build(ingest);
    let (visited, mut test_hops) =
        traverse_reverse_callers(&seeds.seed_nodes, graph, &value, ingest, interner);

    let mut tests = Vec::new();
    let mut seen_test_paths = HashSet::new();

    for direct_test in &seeds.seed_test_files {
        seen_test_paths.insert(direct_test.clone());
        tests.push(AffectedTest::new(direct_test.clone(), 0, true, false));
    }

    for partner in find_partner_tests(&seeds.seed_source_files, ingest, interner) {
        if let Some(row) = tests.iter_mut().find(|t| t.path == partner) {
            row.is_partner = true;
        } else {
            let hops = test_hops.remove(&partner).unwrap_or(0);
            tests.push(AffectedTest::new(partner.clone(), hops, false, true));
        }
        seen_test_paths.insert(partner);
    }

    for importer in assertion_importers(&seeds.seed_source_files, ingest, interner) {
        if seen_test_paths.insert(importer.clone()) {
            let hops = test_hops.remove(&importer).unwrap_or(1);
            tests.push(AffectedTest::new(importer, hops, false, false));
        }
    }

    for (test_path, hops) in test_hops {
        if seen_test_paths.insert(test_path.clone()) {
            tests.push(AffectedTest::new(test_path, hops, false, false));
        }
    }

    tests.sort_by(|a, b| {
        b.is_direct_change
            .cmp(&a.is_direct_change)
            .then_with(|| b.is_partner.cmp(&a.is_partner))
            .then_with(|| a.hops.cmp(&b.hops))
            .then_with(|| a.path.cmp(&b.path))
    });

    let seed_set: HashSet<NodeId> = seeds.seed_nodes.iter().copied().collect();
    let mut impacted: Vec<NodeId> = visited
        .iter()
        .copied()
        .filter(|n| !seed_set.contains(n))
        .collect();
    impacted.sort_unstable();

    AffectedReport {
        changed_spec,
        seeded_by: seeds.seeded_by(),
        seed_count: seeds.seed_nodes.len(),
        seed_test_files: seeds.seed_test_files,
        total_tests: tests.len(),
        reached_symbols: visited.len(),
        script_gates_unmodelled: 0,
        impacted,
        tests,
    }
}
