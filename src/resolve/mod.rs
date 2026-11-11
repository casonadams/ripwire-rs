mod callables;
pub use callables::CallableIndex;
pub mod ladder;
pub mod scip;
mod scopes;
pub mod traits;

pub use ladder::*;
pub use scip::scip_sites;
pub use traits::*;

use crate::domain::{IngestResult, Interner, NodeId, Reference};
use lasso::Spur;
use rayon::prelude::*;
use std::collections::{HashMap, HashSet};

/// Intermediate structure accumulating confidence and reference counts per directed edge.
#[derive(Debug, Default, Clone, Copy, PartialEq)]
pub struct EdgeAcc {
    pub conf_sum: f32,
    pub n_ref: u32,
    /// References that are call flow (not type mentions); an edge with none carries no PageRank weight.
    pub calls: u32,
    pub prov: EdgeProv,
}

/// Resolved graph edges and symbol ambiguity counts.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct ResolutionResult {
    pub edge_accumulators: HashMap<(NodeId, NodeId), EdgeAcc>,
    pub amb_out: Vec<u32>,
    pub unresolved: usize,
    pub external: usize,
    pub declined: usize,
    /// Per reference, in input order: bound to a definition by evidence stronger than its name alone.
    pub bound_refs: Vec<bool>,
}

/// True when evidence beyond the name alone pins the call to one definition other than the caller, so a
/// same-named definition elsewhere is not what it reaches.
fn is_bound_elsewhere(resolved: &ResolvedRef) -> bool {
    !matches!(
        resolved.prov,
        EdgeProv::Split | EdgeProv::Template | EdgeProv::Name
    ) && resolved.targets.iter().any(|&to| to != resolved.from)
}

/// Resolve all call references from an ingestion result into edge accumulators and ambiguity metrics.
fn record_resolved_ref(
    resolved: ResolvedRef,
    edge_accumulators: &mut HashMap<(NodeId, NodeId), EdgeAcc>,
    amb_out: &mut [u32],
) {
    if resolved.is_ambiguous() {
        amb_out[resolved.from as usize] += 1;
    }
    let from = resolved.from;
    for to in resolved.targets.into_iter().filter(|&to| to != from) {
        let acc = edge_accumulators.entry((resolved.from, to)).or_default();
        acc.conf_sum += resolved.conf_per_target;
        acc.n_ref += 1;
        acc.calls += u32::from(!resolved.type_use);
        acc.prov = acc.prov.max(resolved.prov);
    }
}

#[must_use]
pub fn resolve_reference_chunk_with_context<L: ResolutionLookup>(
    chunk: &[Reference],
    lookup: &L,
    num_symbols: usize,
) -> ResolutionResult {
    let mut res = ResolutionResult {
        edge_accumulators: HashMap::new(),
        amb_out: vec![0u32; num_symbols],
        unresolved: 0,
        external: 0,
        declined: 0,
        bound_refs: Vec::with_capacity(chunk.len()),
    };
    for r in chunk {
        let mut bound = false;
        match lookup.classify_ref(r) {
            RefOutcome::Resolved(resolved) => {
                bound = is_bound_elsewhere(&resolved);
                record_resolved_ref(resolved, &mut res.edge_accumulators, &mut res.amb_out);
            }
            RefOutcome::Unresolved => res.unresolved += 1,
            RefOutcome::External => res.external += 1,
            RefOutcome::Declined | RefOutcome::Builtin => res.declined += 1,
            RefOutcome::Ignored => {}
        }
        res.bound_refs.push(bound);
    }
    res
}

#[must_use]
pub fn merge_resolutions(mut a: ResolutionResult, b: ResolutionResult) -> ResolutionResult {
    for (k, acc_b) in b.edge_accumulators {
        let acc_a = a.edge_accumulators.entry(k).or_default();
        acc_a.conf_sum += acc_b.conf_sum;
        acc_a.n_ref += acc_b.n_ref;
        acc_a.calls += acc_b.calls;
        acc_a.prov = acc_a.prov.max(acc_b.prov);
    }
    for (i, val) in b.amb_out.into_iter().enumerate() {
        a.amb_out[i] += val;
    }
    a.unresolved += b.unresolved;
    a.external += b.external;
    a.declined += b.declined;
    a.bound_refs.extend(b.bound_refs);
    a
}

/// Per file and name, the byte spans of the scopes that bind the name as a local variable or parameter.
pub type LocalScopes = HashMap<(u32, Spur), Vec<(u32, u32)>>;

/// Exported name and calling language to the definitions a binding framework exposes.
pub type ForeignIndex = HashMap<(Spur, crate::domain::Lang), Vec<NodeId>>;

/// Call site `(file_id, line, callee)` to what a SCIP index says it reaches.
pub type ScipIndex = HashMap<(u32, u32, Spur), crate::domain::ScipTarget>;

pub struct ResolutionIndex {
    file_dirs: Vec<u32>,
    by_name: HashMap<Spur, Vec<NodeId>>,
    file_scopes: Vec<HashSet<Spur>>,
    local_scopes: LocalScopes,
    module_qualifiers: Vec<HashMap<Spur, u32>>,
    file_imports: Vec<HashSet<u32>>,
    reexports: ReexportTable,
    foreign: ForeignIndex,
    scip: ScipIndex,
    traits: TraitIndex,
    callables: CallableIndex,
}

impl ResolutionIndex {
    #[must_use]
    pub fn build(ingest: &IngestResult, interner: &Interner) -> Self {
        let by_name = index_symbols_by_name(&ingest.symbols);
        let file_dirs = compute_file_dirs(&ingest.files, interner);
        let module_qualifiers = index_module_qualifiers(ingest, interner);
        let qualifier_file = |file: u32, q: Spur| {
            module_qualifiers
                .get(file as usize)
                .and_then(|m| m.get(&q).copied())
        };
        let reexports = index_reexports(ingest, interner);
        let reaches =
            |module: u32, target: u32, name: Spur| reexports.reexports_to(module, target, name);
        let env = TraitEnv {
            symbols: &ingest.symbols,
            file_dirs: &file_dirs,
            interner,
            qualifier_file: &qualifier_file,
            reaches: &reaches,
        };
        let traits = TraitIndex::build(&ingest.implementations, &ingest.files, &env);
        let mut index = Self {
            file_dirs,
            foreign: index_foreign_bindings(ingest, &by_name),
            by_name,
            file_scopes: index_file_scope_mentions(ingest.files.len(), &ingest.references),
            local_scopes: index_local_scopes(ingest),
            module_qualifiers,
            file_imports: index_file_imports(ingest, interner),
            reexports,
            scip: ingest
                .scip_sites
                .iter()
                .map(|s| ((s.file_id, s.line, s.callee), s.target))
                .collect(),
            traits,
            callables: CallableIndex::default(),
        };
        index.callables = CallableIndex::build(ingest, &index.context(ingest, interner));
        index
    }

    #[must_use]
    pub fn trait_implementors(&self, trait_id: NodeId) -> Vec<NodeId> {
        self.traits.implementor_ids(trait_id)
    }

    #[must_use]
    pub fn context<'a>(
        &'a self,
        ingest: &'a IngestResult,
        interner: &'a Interner,
    ) -> ResolutionContext<'a> {
        ResolutionContext {
            symbols: &ingest.symbols,
            file_dirs: &self.file_dirs,
            by_name: &self.by_name,
            interner: Some(interner),
            file_scopes: Some(&self.file_scopes),
            local_scopes: Some(&self.local_scopes),
            module_qualifiers: Some(&self.module_qualifiers),
            file_imports: Some(&self.file_imports),
            reexports: Some(&self.reexports),
            foreign: Some(&self.foreign),
            scip: Some(&self.scip),
            trait_index: Some(&self.traits),
            callables: Some(&self.callables),
        }
    }
}

/// Index `ingest.local_bindings` by file and name.
fn index_local_scopes(ingest: &IngestResult) -> LocalScopes {
    let mut scopes = LocalScopes::new();
    for binding in &ingest.local_bindings {
        scopes
            .entry((binding.file_id, binding.name))
            .or_default()
            .push((binding.scope_start, binding.scope_end));
    }
    scopes
}

/// Bind each exported name to its target: definitions of the target's name and scope in the
/// exporting file's dialect, preferring the exporting file, else one unique definition group.
fn index_foreign_bindings(
    ingest: &IngestResult,
    by_name: &HashMap<Spur, Vec<NodeId>>,
) -> ForeignIndex {
    let mut index = ForeignIndex::new();
    for binding in &ingest.foreign_bindings {
        let Some(file) = ingest.files.get(binding.file_id as usize) else {
            continue;
        };
        let dialect = file.lang.semantics().dialect_group();
        let cands: Vec<NodeId> = by_name
            .get(&binding.target)
            .into_iter()
            .flatten()
            .copied()
            .filter(|&id| {
                let sym = &ingest.symbols[id as usize];
                sym.lang.semantics().dialect_group() == dialect
                    && binding.target_scope.is_none_or(|sc| sym.scope == Some(sc))
            })
            .collect();
        let in_file: Vec<NodeId> = cands
            .iter()
            .copied()
            .filter(|&id| ingest.symbols[id as usize].file_id == binding.file_id)
            .collect();
        let targets = if !in_file.is_empty() {
            in_file
        } else if is_unique_global_group(&cands, &ingest.symbols) {
            cands
        } else {
            continue;
        };
        let entry = index.entry((binding.exported, binding.caller)).or_default();
        entry.extend(targets);
        entry.sort_unstable();
        entry.dedup();
    }
    index
}

pub fn resolve_all_references(ingest: &IngestResult, interner: &Interner) -> ResolutionResult {
    let index = ResolutionIndex::build(ingest, interner);
    let ctx = index.context(ingest, interner);
    let num_symbols = ingest.symbols.len();

    if ingest.references.len() < 256 {
        return resolve_reference_chunk_with_context(&ingest.references, &ctx, num_symbols);
    }
    // Float sums are not associative: merge chunk results in input order, not rayon's split order.
    let chunks: Vec<ResolutionResult> = ingest
        .references
        .par_chunks(512)
        .map(|chunk| resolve_reference_chunk_with_context(chunk, &ctx, num_symbols))
        .collect();
    chunks
        .into_iter()
        .reduce(merge_resolutions)
        .unwrap_or_else(|| resolve_reference_chunk_with_context(&[], &ctx, num_symbols))
}
