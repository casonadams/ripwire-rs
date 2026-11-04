use super::{IngestResult, Interner, NodeId, RefRole, Reference, Symbol};
use lasso::Spur;

/// Unified read-only view over an indexed corpus (`IngestResult` + `Interner`) to simplify caller queries.
pub trait CorpusView<'a> {
    fn ingest(&self) -> &'a IngestResult;
    fn interner(&self) -> &'a Interner;

    #[inline]
    #[must_use]
    fn sym_name(&self, sym: &Symbol) -> &'a str {
        self.interner().try_resolve(&sym.name).unwrap_or("")
    }

    #[inline]
    #[must_use]
    fn sym_scope(&self, sym: &Symbol) -> Option<&'a str> {
        sym.scope
            .as_ref()
            .and_then(|sc| self.interner().try_resolve(sc))
            .filter(|s| !s.is_empty())
    }

    #[inline]
    #[must_use]
    fn sym_path(&self, sym: &Symbol) -> &'a str {
        self.ingest().sym_file_path(sym, self.interner())
    }

    #[inline]
    #[must_use]
    fn file_path(&self, file_id: u32) -> &'a str {
        self.ingest().file_path(file_id, self.interner())
    }

    /// Locate the first symbol matching `symbol_name` and an optional substring `path_hint`.
    #[must_use]
    fn find_symbol_with_hint(
        &self,
        symbol_name: &str,
        path_hint: Option<&str>,
    ) -> Option<&'a Symbol> {
        self.ingest().symbols.iter().find(|s| {
            self.sym_name(s) == symbol_name
                && path_hint.is_none_or(|h| self.sym_path(s).contains(h))
        })
    }

    #[inline]
    #[must_use]
    fn is_test_symbol(&self, sym: &Symbol) -> bool {
        crate::gate::affected::is_test_or_bench_path(self.sym_path(sym))
            || self.sym_name(sym).starts_with("test_")
            || self
                .sym_scope(sym)
                .is_some_and(|sc| sc == "tests" || sc == "test" || sc.ends_with("Tests"))
    }

    #[inline]
    #[must_use]
    fn is_production_symbol(&self, sym: &Symbol) -> bool {
        sym.is_code_symbol() && !self.is_test_symbol(sym)
    }

    /// Compute boolean bitmap of symbols transitively reached from any test symbol via `graph` call edges.
    #[must_use]
    fn compute_test_reachability(&self, graph: &crate::graph::Graph) -> Vec<bool> {
        let ingest = self.ingest();
        let n = ingest.symbols.len();
        let mut reached = vec![false; n];
        let mut queue = std::collections::VecDeque::new();
        for sym in &ingest.symbols {
            let idx = sym.id as usize;
            if idx < n && self.is_test_symbol(sym) {
                reached[idx] = true;
                queue.push_back(sym.id);
            }
        }
        let value = crate::graph::value_reach::ValueReach::build(ingest);
        while let Some(curr) = queue.pop_front() {
            for callee in graph
                .callees(curr)
                .map(|(c, _)| c)
                .chain(value.targets(curr))
            {
                let idx = callee as usize;
                if idx < n && !reached[idx] {
                    reached[idx] = true;
                    queue.push_back(callee);
                }
            }
        }
        reached
    }

    /// Build a fast `path -> file_id` lookup map over all indexed files.
    #[must_use]
    fn file_index_map(&self) -> std::collections::HashMap<&'a str, u32> {
        self.ingest()
            .files
            .iter()
            .map(|f| (self.file_path(f.id), f.id))
            .collect()
    }

    /// Collect sorted, deduplicated `file:line` call-site strings from `caller_id` targeting any symbol in `callee_names`.
    #[must_use]
    fn caller_site_locations(&self, caller_id: NodeId, callee_names: &[Spur]) -> Vec<String> {
        let mut sites: Vec<(u32, u32)> = self
            .ingest()
            .references
            .iter()
            .filter(|r| {
                r.from_symbol == caller_id
                    && matches!(r.role, RefRole::Call | RefRole::Macro)
                    && callee_names.contains(&r.callee_name)
            })
            .map(|r| (r.file_id, r.line))
            .collect();
        sites.sort_unstable();
        sites.dedup();
        sites
            .into_iter()
            .map(|(fid, line)| format!("{}:{line}", self.file_path(fid)))
            .collect()
    }

    /// True if reference `r` targets `query` (either bare symbol/scope name or `Scope::member` / `Scope.member`).
    #[must_use]
    fn reference_matches_query(&self, r: &Reference, query: &str) -> bool {
        let interner = self.interner();
        if let Some((qual_part, leaf_part)) = split_scope_and_member(query) {
            if interner.resolve(&r.callee_name) != leaf_part {
                return false;
            }
            let qual_matches = r
                .qualifier
                .is_some_and(|q| interner.resolve(&q) == qual_part);
            let caller_sc_matches = self
                .ingest()
                .sym(r.from_symbol)
                .and_then(|s| s.scope)
                .is_some_and(|sc| interner.resolve(&sc) == qual_part);
            let unique_def_sc_matches = r.qualifier.is_none()
                && self
                    .ingest()
                    .symbols
                    .iter()
                    .filter(|s| s.name == r.callee_name)
                    .all(|s| s.scope.is_some_and(|sc| interner.resolve(&sc) == qual_part));
            return qual_matches || caller_sc_matches || unique_def_sc_matches;
        }
        interner.resolve(&r.callee_name) == query
            || r.qualifier.is_some_and(|q| interner.resolve(&q) == query)
    }

    /// Resolve sorted, deduplicated file paths that import any file defining `seed_ids` or reference the target symbols.
    #[must_use]
    fn resolve_symbol_importers(&self, seed_ids: &[NodeId]) -> Vec<&'a str> {
        let ingest = self.ingest();
        let seed_files: std::collections::HashSet<u32> = seed_ids
            .iter()
            .filter_map(|&id| ingest.sym(id).map(|s| s.file_id))
            .collect();
        let seed_names: std::collections::HashSet<Spur> = seed_ids
            .iter()
            .filter_map(|&id| ingest.sym(id).map(|s| s.name))
            .collect();
        let file_index = self.file_index_map();
        let mut importers = std::collections::BTreeSet::new();

        for r in &ingest.references {
            if seed_files.contains(&r.file_id) {
                continue;
            }
            let src_path = self.file_path(r.file_id);
            if !src_path.is_empty()
                && is_ref_importing_seed(
                    r,
                    src_path,
                    ingest,
                    self.interner(),
                    &file_index,
                    &seed_files,
                    &seed_names,
                )
            {
                importers.insert(src_path);
            }
        }
        importers.into_iter().collect()
    }

    /// Collect all reference sites matching `query` as `(role_tag, file_path, line, enclosing_sym_name)`.
    #[must_use]
    fn collect_symbol_uses(
        &self,
        query: &str,
    ) -> Vec<(&'static str, &'a str, u32, Option<&'a str>)> {
        let ingest = self.ingest();
        ingest
            .references
            .iter()
            .filter(|r| self.reference_matches_query(r, query))
            .map(|r| {
                let file_path = self.file_path(r.file_id);
                let enclosing = (r.from_symbol != super::NO_NODE)
                    .then(|| ingest.sym(r.from_symbol).map(|s| self.sym_name(s)))
                    .flatten();
                (r.role.tag(), file_path, r.line, enclosing)
            })
            .collect()
    }
}

fn split_scope_and_member(query: &str) -> Option<(&str, &str)> {
    query
        .rsplit_once("::")
        .or_else(|| query.rsplit_once('.'))
        .filter(|(q, m)| !q.is_empty() && !m.is_empty())
}

fn is_ref_importing_seed(
    r: &Reference,
    src_path: &str,
    ingest: &IngestResult,
    interner: &Interner,
    file_index: &std::collections::HashMap<&str, u32>,
    seed_files: &std::collections::HashSet<u32>,
    seed_names: &std::collections::HashSet<Spur>,
) -> bool {
    if r.role != RefRole::Import {
        return false;
    }
    let raw = interner.resolve(&r.callee_name);
    let target_fid = ingest.file(r.file_id).and_then(|f| {
        crate::languages::resolve_import(f.lang, src_path, raw, file_index, &ingest.import_aliases)
    });
    if target_fid.is_some_and(|tf| seed_files.contains(&tf)) {
        return true;
    }
    seed_names.iter().any(|&sn| {
        let sname = interner.resolve(&sn);
        raw == sname || raw.ends_with(&format!("::{sname}"))
    })
}

impl<'a> CorpusView<'a> for (&'a IngestResult, &'a Interner) {
    #[inline]
    fn ingest(&self) -> &'a IngestResult {
        self.0
    }

    #[inline]
    fn interner(&self) -> &'a Interner {
        self.1
    }
}
