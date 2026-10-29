use crate::domain::{FileInfo, Interner, NO_NODE, NodeId, RefRole, Reference, SymKind, Symbol};
use lasso::Spur;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct CandBuffer {
    inline: [NodeId; 8],
    len: usize,
    heap: Vec<NodeId>,
}

impl CandBuffer {
    #[inline]
    #[must_use]
    pub fn new() -> Self {
        Self {
            inline: [0; 8],
            len: 0,
            heap: Vec::new(),
        }
    }

    #[inline]
    pub fn push(&mut self, id: NodeId) {
        if self.heap.is_empty() && self.len < 8 {
            self.inline[self.len] = id;
            self.len += 1;
        } else {
            if self.heap.is_empty() {
                self.heap.reserve(self.len + 8);
                self.heap.extend_from_slice(&self.inline[..self.len]);
            }
            self.heap.push(id);
        }
    }

    #[inline]
    #[must_use]
    pub fn as_slice(&self) -> &[NodeId] {
        if self.heap.is_empty() {
            &self.inline[..self.len]
        } else {
            &self.heap
        }
    }

    #[inline]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        if self.heap.is_empty() {
            self.len == 0
        } else {
            self.heap.is_empty()
        }
    }
}

impl Default for CandBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for CandBuffer {
    type Target = [NodeId];

    #[inline]
    fn deref(&self) -> &[NodeId] {
        self.as_slice()
    }
}

impl AsRef<[NodeId]> for CandBuffer {
    #[inline]
    fn as_ref(&self) -> &[NodeId] {
        self.as_slice()
    }
}

impl<'a> IntoIterator for &'a CandBuffer {
    type Item = &'a NodeId;
    type IntoIter = std::slice::Iter<'a, NodeId>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.as_slice().iter()
    }
}

/// How a resolved call edge was bound, weakest evidence first so `max` keeps the strongest.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default, Serialize, Deserialize,
)]
pub enum EdgeProv {
    /// One arm of a call split across several definitions.
    #[default]
    Split,
    /// Bound to a primary template whose specialization is another definition (which one runs is
    /// unproven).
    Template,
    /// Bound by the name alone: a same-directory or repository-wide match.
    Name,
    /// Bound by lexical evidence: the caller's own file or enclosing type defines it.
    Local,
    /// A type qualifier matched the definition's scope by its last segment only.
    FinalSegment,
    /// The qualifier is a module alias bound by an import of the definition's file.
    Import,
    Typed,
    Binding,
    /// A compiler-backed SCIP index pinned the call site to this definition.
    Scip,
}

impl EdgeProv {
    #[must_use]
    pub fn is_split(self) -> bool {
        self == Self::Split
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Split => "split",
            Self::Template => "template",
            Self::Name => "name",
            Self::Local => "local",
            Self::FinalSegment => "final-segment",
            Self::Import => "import",
            Self::Typed => "typed",
            Self::Binding => "binding",
            Self::Scip => "scip",
        }
    }

    #[must_use]
    pub fn callee_label(self) -> Option<&'static str> {
        match self {
            Self::Template | Self::Name | Self::Local | Self::Typed => None,
            other => Some(other.label()),
        }
    }

    /// True when only the name bound the edge (`via="name"`).
    #[must_use]
    pub fn is_by_name(self) -> bool {
        self == Self::Name
    }

    /// Suffix on a caller row's `of_def=K`: split arm, primary template, or bound by name alone.
    #[must_use]
    pub fn of_def_suffix(self) -> &'static str {
        match self {
            Self::Split => "~s",
            Self::Template => "~t",
            Self::Name => "~n",
            Self::Local
            | Self::FinalSegment
            | Self::Import
            | Self::Typed
            | Self::Binding
            | Self::Scip => "",
        }
    }
}

/// Which evidence narrowed a reference's candidates before the precedence ladder ran.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Narrowing {
    None,
    CallerScope,
    Owner,
    FinalSegment,
    Import,
}

/// Resolved reference outcome with target nodes and per-target confidence.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedRef {
    pub from: NodeId,
    pub targets: Vec<NodeId>,
    pub conf_per_target: f32,
    pub prov: EdgeProv,
    /// A type mention (`RefRole::Type`): navigable as a reference, but no call flow for PageRank.
    pub type_use: bool,
}

impl ResolvedRef {
    #[must_use]
    pub fn is_ambiguous(&self) -> bool {
        self.prov.is_split()
    }

    fn bound(from: NodeId, targets: Vec<NodeId>, prov: EdgeProv) -> Option<Self> {
        if targets.is_empty() {
            return None;
        }
        Some(Self {
            from,
            conf_per_target: 1.0 / targets.len() as f32,
            prov: if targets.len() > 1 {
                EdgeProv::Split
            } else {
                prov
            },
            targets,
            type_use: false,
        })
    }
}

/// Triage classification for a reference during call-graph resolution.
#[derive(Debug, Clone, PartialEq)]
pub enum RefOutcome {
    Ignored,
    Resolved(ResolvedRef),
    Unresolved,
    External,
    /// In-tree candidates exist but no precedence tier could choose among them.
    Declined,
    /// A std/builtin member name whose in-tree namesakes were all rejected.
    Builtin,
}

/// Assign integer directory IDs based on the parent directory of each file.
#[must_use]
pub fn compute_file_dirs(files: &[FileInfo], interner: &Interner) -> Vec<u32> {
    let mut dir_map: HashMap<String, u32> = HashMap::new();
    let mut file_dirs = Vec::with_capacity(files.len());
    for f in files {
        let path = interner.resolve(&f.path);
        let dir = Path::new(path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string();
        let next_id = dir_map.len() as u32;
        let dir_id = *dir_map.entry(dir).or_insert(next_id);
        file_dirs.push(dir_id);
    }
    file_dirs
}

/// Index symbols by their interned name.
#[must_use]
pub fn index_symbols_by_name(symbols: &[Symbol]) -> HashMap<Spur, Vec<NodeId>> {
    let mut by_name: HashMap<Spur, Vec<NodeId>> = HashMap::new();
    for s in symbols {
        by_name.entry(s.name).or_default().push(s.id);
    }
    by_name
}

/// Select candidates via the precedence ladder:
/// Tier 1: Same file (conf = 1.0)
/// Tier 2: Same directory (conf = 0.5)
/// Tier 3: Unique same-language global (conf = 0.2), else dropped.
pub(crate) fn is_unique_global_group(cands: &[NodeId], symbols: &[Symbol]) -> bool {
    let Some(&first) = cands.first() else {
        return false;
    };
    let first_sym = &symbols[first as usize];
    let first_file = first_sym.file_id;
    let first_scope = first_sym.scope;
    cands.iter().all(|&id| {
        let sym = &symbols[id as usize];
        sym.file_id == first_file
            && (sym.scope == first_scope
                || sym.scope == Some(first_sym.name)
                || first_scope == Some(sym.name))
    })
}

/// Precedence-ladder rung that chose a reference's targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tier {
    SameFile,
    SameDir,
    Global,
}

impl Tier {
    #[must_use]
    pub fn confidence(self) -> f32 {
        match self {
            Self::SameFile => 1.0,
            Self::SameDir => 0.5,
            Self::Global => 0.2,
        }
    }
}

pub fn select_precedence_tier(
    cands: &[NodeId],
    caller_file_id: u32,
    caller_dir_id: u32,
    symbols: &[Symbol],
    file_dirs: &[u32],
) -> Option<(Vec<NodeId>, Tier)> {
    let same_file: Vec<NodeId> = cands
        .iter()
        .copied()
        .filter(|&id| symbols[id as usize].file_id == caller_file_id)
        .collect();
    if !same_file.is_empty() {
        return Some((same_file, Tier::SameFile));
    }

    let same_dir: Vec<NodeId> = cands
        .iter()
        .copied()
        .filter(|&id| file_dirs.get(symbols[id as usize].file_id as usize) == Some(&caller_dir_id))
        .collect();
    if !same_dir.is_empty() {
        return (same_dir.len() <= 4).then_some((same_dir, Tier::SameDir));
    }

    is_unique_global_group(cands, symbols).then(|| (cands.to_vec(), Tier::Global))
}

fn is_method_std_member_blocked<L: ResolutionLookup + ?Sized>(
    lookup: &L,
    r: &Reference,
    sym: &Symbol,
    caller_scope: Option<Spur>,
) -> bool {
    let Some(sym_sc) = sym.scope else {
        return true;
    };
    if caller_scope == Some(sym_sc) {
        return false;
    }
    !(caller_scope.is_none()
        && sym.file_id != r.file_id
        && lookup.file_mentions_scope(r.file_id, sym_sc))
}

fn is_unpinned_std_member_blocked<L: ResolutionLookup + ?Sized>(
    lookup: &L,
    r: &Reference,
    sym: &Symbol,
    caller_scope: Option<Spur>,
    callee_str: &str,
) -> bool {
    if !sym.lang.semantics().is_std_or_builtin_member(callee_str) {
        return false;
    }
    if let Some(q) = r.qualifier {
        return sym.scope != Some(q);
    }
    match sym.kind {
        crate::domain::SymKind::Field | crate::domain::SymKind::Var => true,
        crate::domain::SymKind::Method | crate::domain::SymKind::Function
            if lookup.is_type_member(sym) =>
        {
            is_method_std_member_blocked(lookup, r, sym, caller_scope)
        }
        crate::domain::SymKind::Function => {
            sym.file_id != r.file_id && !lookup.file_imports_file(r.file_id, sym.file_id)
        }
        _ => false,
    }
}

fn filter_matching_scope(cands: &CandBuffer, symbols: &[Symbol], q: Spur) -> CandBuffer {
    let mut qualified = CandBuffer::default();
    for &id in cands {
        let s = &symbols[id as usize];
        if s.scope == Some(q) || s.name == q {
            qualified.push(id);
        }
    }
    qualified
}

fn filter_in_file(cands: &CandBuffer, symbols: &[Symbol], file_id: u32) -> CandBuffer {
    let mut in_file = CandBuffer::default();
    for &id in cands {
        if symbols[id as usize].file_id == file_id {
            in_file.push(id);
        }
    }
    in_file
}

fn filter_unscoped(cands: &CandBuffer, symbols: &[Symbol]) -> CandBuffer {
    let mut free = CandBuffer::default();
    for &id in cands {
        if symbols[id as usize].scope.is_none() {
            free.push(id);
        }
    }
    free
}

/// Index per-file referenced identifiers and qualifiers for cross-file receiver evidence.
#[must_use]
pub fn index_file_scope_mentions(
    files_len: usize,
    references: &[Reference],
) -> Vec<std::collections::HashSet<Spur>> {
    let mut sets = vec![std::collections::HashSet::new(); files_len];
    for r in references {
        if let Some(set) = sets.get_mut(r.file_id as usize) {
            set.insert(r.callee_name);
            if let Some(q) = r.qualifier {
                set.insert(q);
            }
        }
    }
    sets
}

/// Per file, the identifiers its imports bind to an indexed module file (`import x as y` binds `y`).
#[must_use]
pub fn index_module_qualifiers(
    ingest: &crate::domain::IngestResult,
    interner: &Interner,
) -> Vec<HashMap<Spur, u32>> {
    let file_index: HashMap<&str, u32> = ingest
        .files
        .iter()
        .map(|f| (interner.resolve(&f.path), f.id))
        .collect();
    let mut maps = vec![HashMap::new(); ingest.files.len()];
    for r in ingest
        .references
        .iter()
        .filter(|r| r.role == RefRole::Import)
    {
        let raw = interner.resolve(&r.callee_name);
        let Some(importer) = ingest.files.get(r.file_id as usize) else {
            continue;
        };
        let importer_path = interner.resolve(&importer.path);
        for (target, bound) in r.lang.semantics().import_module_bindings(raw) {
            let Some(bound) = interner.get(&bound) else {
                continue;
            };
            if let Some(file) = crate::languages::resolve_import(
                r.lang,
                importer_path,
                &target,
                &file_index,
                &ingest.import_aliases,
            ) {
                maps[r.file_id as usize].insert(bound, file);
            }
        }
    }
    for symbol in ingest.symbols.iter().filter(|symbol| !symbol.has_body()) {
        let Some(raw) = symbol
            .lang
            .semantics()
            .symbol_implied_import(symbol.kind, interner.resolve(&symbol.name))
        else {
            continue;
        };
        let Some(importer) = ingest.files.get(symbol.file_id as usize) else {
            continue;
        };
        if let Some(target) = crate::languages::resolve_import(
            symbol.lang,
            interner.resolve(&importer.path),
            &raw,
            &file_index,
            &ingest.import_aliases,
        ) {
            maps[symbol.file_id as usize].insert(symbol.name, target);
        }
    }
    bind_path_qualifiers(ingest, interner, &file_index, &mut maps);
    maps
}

fn bind_path_qualifiers(
    ingest: &crate::domain::IngestResult,
    interner: &Interner,
    file_index: &HashMap<&str, u32>,
    maps: &mut [HashMap<Spur, u32>],
) {
    for r in ingest
        .references
        .iter()
        .filter(|r| r.lang.semantics().keeps_path_qualifier())
    {
        let (Some(q), Some(importer)) = (r.qualifier, ingest.files.get(r.file_id as usize)) else {
            continue;
        };
        let map = &mut maps[r.file_id as usize];
        let text = interner.resolve(&q);
        if map.contains_key(&q) || !text.contains("::") {
            continue;
        }
        if let Some(file) = crate::languages::resolve_import(
            r.lang,
            interner.resolve(&importer.path),
            text,
            file_index,
            &ingest.import_aliases,
        ) {
            map.insert(q, file);
        }
    }
}

/// Per file, the indexed files its imports resolve to; resolution is the importing language's own.
#[must_use]
pub fn index_file_imports(
    ingest: &crate::domain::IngestResult,
    interner: &Interner,
) -> Vec<std::collections::HashSet<u32>> {
    let file_index: HashMap<&str, u32> = ingest
        .files
        .iter()
        .map(|f| (interner.resolve(&f.path), f.id))
        .collect();
    let mut sets = vec![std::collections::HashSet::new(); ingest.files.len()];
    for r in ingest
        .references
        .iter()
        .filter(|r| r.role == RefRole::Import)
    {
        let Some(importer) = ingest.files.get(r.file_id as usize) else {
            continue;
        };
        if let Some(target) = crate::languages::resolve_import(
            r.lang,
            interner.resolve(&importer.path),
            interner.resolve(&r.callee_name),
            &file_index,
            &ingest.import_aliases,
        ) {
            sets[r.file_id as usize].insert(target);
        }
    }
    sets
}

fn edge_prov(target_count: usize, narrowing: Narrowing, tier: Tier) -> EdgeProv {
    if target_count > 1 {
        return EdgeProv::Split;
    }
    match (narrowing, tier) {
        (Narrowing::Import, _) => EdgeProv::Import,
        (Narrowing::FinalSegment, _) => EdgeProv::FinalSegment,
        (Narrowing::CallerScope | Narrowing::Owner, _) | (Narrowing::None, Tier::SameFile) => {
            EdgeProv::Local
        }
        (Narrowing::None, Tier::SameDir | Tier::Global) => EdgeProv::Name,
    }
}

/// True for a bodyless declaration in a language that separates declarations from definitions.
#[must_use]
pub fn is_declaration(sym: &Symbol) -> bool {
    !sym.has_body() && sym.lang.semantics().has_declarations()
}

/// The bodied definition among `cands` that bodyless `decl` declares, by its language's tie rule.
#[must_use]
pub fn tied_definition(decl: &Symbol, cands: &[NodeId], symbols: &[Symbol]) -> Option<NodeId> {
    if !is_declaration(decl) {
        return None;
    }
    let semantics = decl.lang.semantics();
    cands.iter().copied().find(|&id| {
        let def = &symbols[id as usize];
        def.has_body()
            && def.name == decl.name
            && def.lang.semantics().dialect_group() == semantics.dialect_group()
            && semantics.declaration_ties(decl, def)
    })
}

/// True when bodyless `decl` is a contract a same-named bodied candidate implements, in a language
/// whose declarations do not tie to implementations by name (TypeScript interface members). By-name
/// calls then reach the implementation, not the contract; typed calls still reach the contract.
fn yields_to_definition(decl: &Symbol, cands: &[NodeId], symbols: &[Symbol]) -> bool {
    decl.lang.semantics().declarations_yield_to_definitions()
        && is_declaration(decl)
        && cands
            .iter()
            .any(|&id| symbols[id as usize].has_body() && symbols[id as usize].name == decl.name)
}

/// The one candidate whose scope every other candidate's scope specializes, if any.
fn primary_template(defs: &[NodeId], symbols: &[Symbol], interner: &Interner) -> Option<NodeId> {
    if defs.len() < 2 {
        return None;
    }
    let scope = |id: NodeId| symbols[id as usize].scope.map(|s| interner.resolve(&s));
    defs.iter().copied().find(|&primary| {
        let Some(primary_scope) = scope(primary) else {
            return false;
        };
        let semantics = symbols[primary as usize].lang.semantics();
        defs.iter().all(|&other| {
            other == primary
                || scope(other).is_some_and(|s| semantics.is_specialization_of(s, primary_scope))
        })
    })
}

fn is_non_table_read_write(r: &Reference) -> bool {
    matches!(r.role, RefRole::Read | RefRole::Write)
        && !r.lang.semantics().is_table_read_write_target()
}

fn classify_rejected_with_interner(r: &Reference, interner: &Interner) -> RefOutcome {
    if let Some(recv) = r.receiver_type
        && let Some(recv_str) = interner.try_resolve(&recv)
        && (recv_str.contains("::") || r.lang.semantics().is_std_or_builtin_qualifier(recv_str))
    {
        return RefOutcome::External;
    }
    if r.qualifier
        .and_then(|q| interner.try_resolve(&q))
        .is_some_and(|qs| r.lang.semantics().is_std_or_builtin_qualifier(qs))
    {
        return RefOutcome::External;
    }
    if r.qualifier.is_some() && r.lang.semantics().qualifier_must_be_indexed() {
        return RefOutcome::External;
    }
    if interner
        .try_resolve(&r.callee_name)
        .is_some_and(|cs| r.lang.semantics().is_std_or_builtin_member(cs))
    {
        return RefOutcome::Builtin;
    }
    RefOutcome::Unresolved
}

fn classify_rejected_candidates(r: &Reference, interner: Option<&Interner>) -> RefOutcome {
    if matches!(r.role, RefRole::Read | RefRole::Write) {
        return if r.lang.semantics().is_table_read_write_target() {
            RefOutcome::External
        } else {
            RefOutcome::Ignored
        };
    }
    interner.map_or(RefOutcome::Unresolved, |i| {
        classify_rejected_with_interner(r, i)
    })
}

/// Trait encapsulating symbol table and directory context for call resolution.
pub trait ResolutionLookup {
    fn symbols(&self) -> &[Symbol];
    fn file_dirs(&self) -> &[u32];
    fn by_name(&self) -> &HashMap<Spur, Vec<NodeId>>;
    fn interner(&self) -> Option<&Interner> {
        None
    }
    fn file_mentions_scope(&self, _file_id: u32, _scope: Spur) -> bool {
        false
    }
    /// True when `file_id` imports the indexed file `target`.
    fn file_imports_file(&self, _file_id: u32, _target: u32) -> bool {
        false
    }
    fn qualifier_module_file(&self, _file_id: u32, _qualifier: Spur) -> Option<u32> {
        None
    }
    fn trait_index(&self) -> Option<&crate::resolve::TraitIndex> {
        None
    }

    /// The indexed type a receiver's declared type names, as seen from the reference's file.
    #[must_use]
    fn receiver_owner(&self, r: &Reference, recv: Spur) -> Option<NodeId> {
        let interner = self.interner()?;
        let trait_idx = self.trait_index()?;
        let text = interner.try_resolve(&recv)?;
        let qualifier_file = |file: u32, q: Spur| self.qualifier_module_file(file, q);
        let env = crate::resolve::TraitEnv {
            symbols: self.symbols(),
            file_dirs: self.file_dirs(),
            interner,
            qualifier_file: &qualifier_file,
        };
        trait_idx.resolve_receiver_owner(text, r.file_id, r.start_byte, &env)
    }

    /// True when a typed receiver names no indexed type and the language lets its call use names.
    #[must_use]
    fn untyped_receiver_uses_names(&self, r: &Reference, recv: Spur) -> bool {
        let Some(text) = self.interner().and_then(|i| i.try_resolve(&recv)) else {
            return false;
        };
        r.lang.semantics().unindexed_receiver_type_uses_names(text)
            && self.receiver_owner(r, recv).is_none()
    }

    #[must_use]
    fn external_owner_candidates(&self, r: &Reference, recv: Spur) -> Option<CandBuffer> {
        let trait_idx = self.trait_index()?;
        let text = self.interner()?.try_resolve(&recv)?;
        self.owner_compatible(
            r,
            trait_idx.external_subject_targets(text, r.callee_name, self.symbols()),
        )
    }

    /// Methods a receiver of a named owner type may reach. An interface owner dispatches to its
    /// implementors and keeps its own declaration; a concrete owner prefers its inherent methods
    /// and otherwise uses trait impls. Same-named methods of unrelated types are never reached.
    #[must_use]
    fn owner_candidates(&self, r: &Reference, recv: Spur) -> Option<CandBuffer> {
        let trait_idx = self.trait_index()?;
        let Some(owner) = self.receiver_owner(r, recv) else {
            return self.external_owner_candidates(r, recv);
        };
        let symbols = self.symbols();
        let found: Vec<NodeId> = if symbols[owner as usize].kind == SymKind::Interface {
            let mut found = trait_idx.dispatch(owner, r.callee_name, symbols);
            if r.lang.semantics().interface_calls_keep_declaration() {
                found.extend(trait_idx.declared_methods(owner, r.callee_name, symbols));
            }
            found.sort_unstable();
            found.dedup();
            found
        } else {
            let all = trait_idx.subject_targets(owner, r.callee_name, symbols);
            let compatible: Vec<NodeId> = all
                .into_iter()
                .filter(|&id| self.is_owner_compatible(r, &symbols[id as usize]))
                .collect();
            let inherent: Vec<NodeId> = compatible
                .iter()
                .copied()
                .filter(|&id| !trait_idx.is_non_inherent(id))
                .collect();
            if inherent.is_empty() {
                compatible
            } else {
                inherent
            }
        };
        self.owner_compatible(r, found)
    }

    #[must_use]
    fn owner_compatible(
        &self,
        r: &Reference,
        ids: impl IntoIterator<Item = NodeId>,
    ) -> Option<CandBuffer> {
        let symbols = self.symbols();
        let mut out = CandBuffer::default();
        for id in ids {
            if self.is_owner_compatible(r, &symbols[id as usize]) {
                out.push(id);
            }
        }
        (!out.is_empty()).then_some(out)
    }

    #[must_use]
    fn is_compatible_target(&self, r: &Reference, sym: &Symbol) -> bool {
        if sym.lang.semantics().is_table_read_write_target()
            && matches!(r.role, RefRole::Read | RefRole::Write)
        {
            return true;
        }
        if sym.kind == crate::domain::SymKind::Section
            && matches!(r.role, RefRole::Call | RefRole::Macro)
        {
            return false;
        }
        if r.member_call
            && r.role == RefRole::Call
            && sym.kind == crate::domain::SymKind::Var
            && r.lang.semantics().member_call_targets_callables_only()
        {
            // `res.json()` calls a property of an unknown receiver; a same-named variable is no callee.
            return false;
        }
        if r.role == RefRole::Call
            && r.lang.semantics().bare_calls_skip_type_members()
            && !self.bare_call_reachable(r, sym)
        {
            return false;
        }
        if let Some(interner) = self.interner() {
            let callee_str = interner.try_resolve(&r.callee_name).unwrap_or("");
            let caller_scope = self
                .symbols()
                .get(r.from_symbol as usize)
                .and_then(|cs| cs.scope);
            if is_unpinned_std_member_blocked(self, r, sym, caller_scope, callee_str) {
                return false;
            }
        }
        self.is_owner_compatible(r, sym)
    }

    /// A bare call reaches `sym` unless it is a member of a type. A definition nested in a function
    /// body is no member: it is in lexical scope wherever that function's body is.
    #[must_use]
    fn bare_call_reachable(&self, r: &Reference, sym: &Symbol) -> bool {
        if r.qualifier.is_some() || r.member_call || !self.type_scoped(sym) {
            return true;
        }
        self.enclosing_function(sym).is_some_and(|f| {
            f.file_id == r.file_id && (f.sig_start_byte..f.end_byte).contains(&r.start_byte)
        })
    }

    /// Whether `sym`'s scope names an indexed type.
    #[must_use]
    fn type_scoped(&self, sym: &Symbol) -> bool {
        let symbols = self.symbols();
        sym.scope.is_some_and(|scope| {
            self.by_name().get(&scope).is_some_and(|ids| {
                ids.iter()
                    .any(|&id| symbols[id as usize].kind.is_type_container())
            })
        })
    }

    /// A member of a type: scoped to one and not nested in a function body. Python tags its methods
    /// as functions, so the kind alone does not say this.
    #[must_use]
    fn is_type_member(&self, sym: &Symbol) -> bool {
        sym.kind == crate::domain::SymKind::Method
            || (self.type_scoped(sym) && self.enclosing_function(sym).is_none())
    }

    /// The innermost function or method of `sym`'s file whose span encloses `sym`. A file's symbols
    /// hold a contiguous id range, so only that range is scanned.
    #[must_use]
    fn enclosing_function(&self, sym: &Symbol) -> Option<&Symbol> {
        let symbols = self.symbols();
        let id = sym.id as usize;
        let same_file = |i: &usize| symbols[*i].file_id == sym.file_id;
        let first = (0..id).rev().take_while(same_file).last().unwrap_or(id);
        let last = (id..symbols.len())
            .take_while(same_file)
            .last()
            .unwrap_or(id);
        symbols[first..=last]
            .iter()
            .filter(|s| {
                s.id != sym.id
                    && matches!(
                        s.kind,
                        crate::domain::SymKind::Function | crate::domain::SymKind::Method
                    )
                    && s.sig_start_byte <= sym.sig_start_byte
                    && sym.end_byte <= s.end_byte
            })
            .min_by_key(|s| s.end_byte - s.sig_start_byte)
    }

    /// Language and arity compatibility, without the std-member name filter.
    #[must_use]
    fn is_owner_compatible(&self, r: &Reference, sym: &Symbol) -> bool {
        let arity_ok = sym.kind == crate::domain::SymKind::Macro
            || !r.arg_count_known
            || r.arg_count <= sym.params
            || (sym.params == 0 && !sym.is_callable());
        sym.lang.is_compatible(r.lang) && arity_ok && !self.needs_missing_self(r, sym)
    }

    #[must_use]
    fn needs_missing_self(&self, r: &Reference, sym: &Symbol) -> bool {
        r.member_call
            && r.qualifier.is_none()
            && sym.is_callable()
            && !sym.takes_self
            && r.lang.semantics().member_call_needs_self()
    }

    #[must_use]
    fn inherited_candidates(&self, r: &Reference, scope: Spur) -> Option<CandBuffer> {
        if !r.lang.semantics().has_class_inheritance() {
            return None;
        }
        let trait_idx = self.trait_index()?;
        let class = self.receiver_owner(r, scope)?;
        self.owner_compatible(
            r,
            trait_idx.subject_targets(class, r.callee_name, self.symbols()),
        )
    }

    /// Candidates a qualified reference can mean: those scoped by the qualifier, else those in the module
    /// file the qualifier is bound to by an import, else all of them unless the qualifier is std/builtin.
    #[must_use]
    fn narrow_by_qualifier(
        &self,
        cands: CandBuffer,
        r: &Reference,
        q: Spur,
    ) -> Option<(CandBuffer, Narrowing)> {
        let symbols = self.symbols();
        let qualified = filter_matching_scope(&cands, symbols, q);
        if !qualified.is_empty() {
            let caller_scope = symbols.get(r.from_symbol as usize).and_then(|cs| cs.scope);
            let narrowing = if caller_scope == Some(q) {
                Narrowing::CallerScope
            } else {
                Narrowing::FinalSegment
            };
            return Some((qualified, narrowing));
        }
        if let Some(inherited) = self.inherited_candidates(r, q) {
            return Some((inherited, Narrowing::Owner));
        }
        if let Some(module_file) = self.qualifier_module_file(r.file_id, q) {
            let in_module = filter_in_file(&cands, symbols, module_file);
            let free = filter_unscoped(&in_module, symbols);
            if !free.is_empty() {
                return Some((free, Narrowing::Import));
            }
            if !in_module.is_empty() {
                return Some((in_module, Narrowing::Import));
            }
        }
        let is_std_qual = self
            .interner()
            .and_then(|i| i.try_resolve(&q))
            .is_some_and(|qs| r.lang.semantics().is_std_or_builtin_qualifier(qs));
        (!is_std_qual && !r.lang.semantics().qualifier_must_be_indexed())
            .then_some((cands, Narrowing::None))
    }

    #[must_use]
    fn apply_qualifier_or_caller_scope(
        &self,
        cands: CandBuffer,
        r: &Reference,
    ) -> Option<(CandBuffer, Narrowing)> {
        let symbols = self.symbols();
        if let Some(q) = r.qualifier {
            return self.narrow_by_qualifier(cands, r, q);
        }
        if let Some(caller_sc) = symbols.get(r.from_symbol as usize).and_then(|cs| cs.scope) {
            let qualified = filter_matching_scope(&cands, symbols, caller_sc);
            if !qualified.is_empty() {
                return Some((qualified, Narrowing::CallerScope));
            }
            if let Some(inherited) = self.inherited_candidates(r, caller_sc) {
                return Some((inherited, Narrowing::Owner));
            }
        }
        Some((cands, Narrowing::None))
    }

    #[must_use]
    fn filter_ref_candidates(
        &self,
        all_cands: &[NodeId],
        r: &Reference,
    ) -> Option<(CandBuffer, Narrowing)> {
        let (cands, narrowing) = self.compatible_candidates(all_cands, r)?;
        Some((self.prefer_exact_arity(r, cands), narrowing))
    }

    #[must_use]
    fn prefer_exact_arity(&self, r: &Reference, cands: CandBuffer) -> CandBuffer {
        if !r.arg_count_known || cands.len() < 2 || !r.lang.semantics().overloads_by_arity() {
            return cands;
        }
        let symbols = self.symbols();
        let exact = |id: NodeId| {
            let sym = &symbols[id as usize];
            sym.is_callable() && sym.params == r.arg_count
        };
        if !cands.iter().any(|&id| exact(id)) {
            return cands;
        }
        let mut out = CandBuffer::default();
        for &id in cands.iter().filter(|&&id| exact(id)) {
            out.push(id);
        }
        out
    }

    #[must_use]
    fn compatible_candidates(
        &self,
        all_cands: &[NodeId],
        r: &Reference,
    ) -> Option<(CandBuffer, Narrowing)> {
        if let Some(recv) = r.receiver_type {
            if let Some(cands) = self.owner_candidates(r, recv) {
                return Some((cands, Narrowing::Owner));
            }
            if !self.untyped_receiver_uses_names(r, recv)
                && !r.lang.semantics().typed_receiver_miss_uses_names()
            {
                return None;
            }
        }
        let symbols = self.symbols();
        let mut cands = CandBuffer::default();
        for &id in all_cands {
            if self.is_compatible_target(r, &symbols[id as usize]) {
                cands.push(id);
            }
        }
        if cands.is_empty() {
            return None;
        }
        self.apply_qualifier_or_caller_scope(cands, r)
    }

    #[must_use]
    fn is_interface_scoped_method(&self, sym: &Symbol) -> bool {
        let Some(sc) = sym.scope else {
            return false;
        };
        let symbols = self.symbols();
        self.by_name().get(&sc).is_some_and(|ids| {
            ids.iter()
                .any(|&id| symbols[id as usize].kind == crate::domain::SymKind::Interface)
        })
    }

    #[must_use]
    fn select_interface_contract_tier(&self, cands: &[NodeId]) -> Option<(Vec<NodeId>, Tier)> {
        let symbols = self.symbols();
        let iface_cands: Vec<NodeId> = cands
            .iter()
            .copied()
            .filter(|&id| self.is_interface_scoped_method(&symbols[id as usize]))
            .collect();
        is_unique_global_group(&iface_cands, symbols).then_some((iface_cands, Tier::Global))
    }

    #[must_use]
    fn with_interface_implementors(&self, r: &Reference, mut targets: Vec<NodeId>) -> Vec<NodeId> {
        let (Some(trait_idx), true) = (self.trait_index(), r.member_call) else {
            return targets;
        };
        let symbols = self.symbols();
        let owners: Vec<NodeId> = targets
            .iter()
            .filter_map(|&id| symbols[id as usize].scope)
            .filter_map(|scope| self.by_name().get(&scope))
            .flatten()
            .copied()
            .filter(|&id| symbols[id as usize].kind == crate::domain::SymKind::Interface)
            .collect();
        for owner in owners {
            if let Some(found) =
                self.owner_compatible(r, trait_idx.dispatch(owner, r.callee_name, symbols))
            {
                targets.extend_from_slice(found.as_slice());
            }
        }
        targets.sort_unstable();
        targets.dedup();
        targets
    }

    #[must_use]
    fn is_call_like_ref(&self, r: &Reference) -> bool {
        if r.from_symbol == NO_NODE {
            return false;
        }
        match r.role {
            RefRole::Call | RefRole::Macro => true,
            RefRole::Read | RefRole::Write => r.lang.semantics().is_table_read_write_target(),
            RefRole::Type => r.lang.semantics().type_mentions_are_edges(),
            _ => false,
        }
    }

    #[must_use]
    fn classify_ref(&self, r: &Reference) -> RefOutcome {
        if !self.is_call_like_ref(r) {
            return RefOutcome::Ignored;
        }
        match self.classify_target(r) {
            RefOutcome::Resolved(mut resolved) => {
                resolved.type_use = r.role == RefRole::Type;
                RefOutcome::Resolved(resolved)
            }
            other => other,
        }
    }

    /// Targets a binding framework exposes to `caller` code under `name`.
    fn foreign_targets(&self, _name: Spur, _caller: crate::domain::Lang) -> Option<&[NodeId]> {
        None
    }

    /// What a SCIP index says the call site reaches.
    fn scip_target(&self, _r: &Reference) -> Option<crate::domain::ScipTarget> {
        None
    }

    /// A SCIP-pinned site wins outright; a call no same-language definition answers may still
    /// reach a definition a binding framework exposed to the caller's language under that name.
    fn classify_target(&self, r: &Reference) -> RefOutcome {
        match self.scip_target(r) {
            Some(crate::domain::ScipTarget::Def(target)) if target != r.from_symbol => {
                return RefOutcome::Resolved(ResolvedRef {
                    from: r.from_symbol,
                    targets: vec![target],
                    conf_per_target: 1.0,
                    prov: EdgeProv::Scip,
                    type_use: false,
                });
            }
            Some(crate::domain::ScipTarget::External) => return RefOutcome::External,
            _ => {}
        }
        if r.callable_field {
            return self.classify_callable(r);
        }
        let outcome = self.classify_by_name(r);
        match outcome {
            RefOutcome::External
            | RefOutcome::Unresolved
            | RefOutcome::Declined
            | RefOutcome::Builtin => {
                if r.macro_arg {
                    return RefOutcome::Ignored;
                }
                self.classify_foreign(r).unwrap_or(outcome)
            }
            RefOutcome::Ignored | RefOutcome::Resolved(_) => outcome,
        }
    }

    fn callable_targets(&self, _r: &Reference) -> Option<&[NodeId]> {
        None
    }

    fn classify_callable(&self, r: &Reference) -> RefOutcome {
        let Some(candidates) = self.callable_targets(r) else {
            return RefOutcome::Declined;
        };
        let targets: Vec<_> = candidates
            .iter()
            .copied()
            .filter(|&id| {
                let symbol = &self.symbols()[id as usize];
                symbol.lang.is_compatible(r.lang)
                    && (!r.arg_count_known || r.arg_count <= symbol.params)
            })
            .collect();
        ResolvedRef::bound(r.from_symbol, targets, EdgeProv::Binding)
            .map_or(RefOutcome::Declined, RefOutcome::Resolved)
    }

    fn classify_foreign(&self, r: &Reference) -> Option<RefOutcome> {
        let targets: Vec<NodeId> = self
            .foreign_targets(r.callee_name, r.lang)?
            .iter()
            .copied()
            .filter(|&target| target != r.from_symbol)
            .collect();
        ResolvedRef::bound(r.from_symbol, targets, EdgeProv::Binding).map(RefOutcome::Resolved)
    }

    /// Candidates with tied declarations folded into their definition, and a primary template
    /// chosen over its specializations (the flag reports that choice).
    fn collapse_definition_groups(&self, cands: &[NodeId]) -> (Vec<NodeId>, bool) {
        let symbols = self.symbols();
        let defs: Vec<NodeId> = cands
            .iter()
            .copied()
            .filter(|&id| tied_definition(&symbols[id as usize], cands, symbols).is_none())
            .filter(|&id| !yields_to_definition(&symbols[id as usize], cands, symbols))
            .collect();
        let primary = self
            .interner()
            .and_then(|interner| primary_template(&defs, symbols, interner));
        match primary {
            Some(primary) => (vec![primary], true),
            None => (defs, false),
        }
    }

    fn classify_by_name(&self, r: &Reference) -> RefOutcome {
        let Some(named) = self.by_name().get(&r.callee_name) else {
            return RefOutcome::External;
        };
        let symbols = self.symbols();
        let without_contracts: Vec<NodeId>;
        let all_cands: &[NodeId] = if named.iter().any(|&id| symbols[id as usize].contract_member) {
            without_contracts = named
                .iter()
                .copied()
                .filter(|&id| !symbols[id as usize].contract_member)
                .collect();
            &without_contracts
        } else {
            named
        };
        let Some((cands, narrowing)) = self.filter_ref_candidates(all_cands, r) else {
            return classify_rejected_candidates(r, self.interner());
        };
        if narrowing == Narrowing::Owner {
            let targets = cands
                .iter()
                .copied()
                .filter(|&id| id != r.from_symbol)
                .collect();
            return ResolvedRef::bound(r.from_symbol, targets, EdgeProv::Typed)
                .map_or(RefOutcome::Ignored, RefOutcome::Resolved);
        }
        let (cands, template) = self.collapse_definition_groups(&cands);
        let caller_file_id = r.file_id;
        let caller_dir_id = self
            .file_dirs()
            .get(caller_file_id as usize)
            .copied()
            .unwrap_or(u32::MAX);
        let Some((tier_targets, tier)) = select_precedence_tier(
            &cands,
            caller_file_id,
            caller_dir_id,
            self.symbols(),
            self.file_dirs(),
        )
        .or_else(|| {
            self.select_interface_contract_tier(&cands)
                .map(|(targets, tier)| (self.with_interface_implementors(r, targets), tier))
        }) else {
            return if is_non_table_read_write(r) {
                RefOutcome::Ignored
            } else {
                RefOutcome::Declined
            };
        };
        self.resolve_targets(r, tier_targets, narrowing, tier, template)
    }

    fn resolve_targets(
        &self,
        r: &Reference,
        targets: Vec<NodeId>,
        narrowing: Narrowing,
        tier: Tier,
        template: bool,
    ) -> RefOutcome {
        let real_targets: Vec<NodeId> = targets
            .into_iter()
            .filter(|&target| target != r.from_symbol)
            .collect();
        if real_targets.is_empty() {
            return RefOutcome::Ignored;
        }
        let prov = if template {
            EdgeProv::Template
        } else {
            edge_prov(real_targets.len(), narrowing, tier)
        };
        let conf_per_target = tier.confidence() / (real_targets.len() as f32);
        RefOutcome::Resolved(ResolvedRef {
            from: r.from_symbol,
            targets: real_targets,
            conf_per_target,
            prov,
            type_use: false,
        })
    }
}

/// Concrete lookup context bundling symbol tables, directory mapping, and optional interner.
pub struct ResolutionContext<'a> {
    pub symbols: &'a [Symbol],
    pub file_dirs: &'a [u32],
    pub by_name: &'a HashMap<Spur, Vec<NodeId>>,
    pub interner: Option<&'a Interner>,
    pub file_scopes: Option<&'a [std::collections::HashSet<Spur>]>,
    pub module_qualifiers: Option<&'a [HashMap<Spur, u32>]>,
    pub file_imports: Option<&'a [std::collections::HashSet<u32>]>,
    pub foreign: Option<&'a HashMap<(Spur, crate::domain::Lang), Vec<NodeId>>>,
    pub scip: Option<&'a HashMap<(u32, u32, Spur), crate::domain::ScipTarget>>,
    pub trait_index: Option<&'a crate::resolve::TraitIndex>,
    pub callables: Option<&'a crate::resolve::CallableIndex>,
}

impl ResolutionLookup for ResolutionContext<'_> {
    #[inline]
    fn symbols(&self) -> &[Symbol] {
        self.symbols
    }

    #[inline]
    fn file_dirs(&self) -> &[u32] {
        self.file_dirs
    }

    #[inline]
    fn by_name(&self) -> &HashMap<Spur, Vec<NodeId>> {
        self.by_name
    }

    #[inline]
    fn interner(&self) -> Option<&Interner> {
        self.interner
    }

    #[inline]
    fn foreign_targets(&self, name: Spur, caller: crate::domain::Lang) -> Option<&[NodeId]> {
        self.foreign?.get(&(name, caller)).map(Vec::as_slice)
    }

    fn callable_targets(&self, r: &Reference) -> Option<&[NodeId]> {
        self.callables?.targets(r, self)
    }

    #[inline]
    fn scip_target(&self, r: &Reference) -> Option<crate::domain::ScipTarget> {
        self.scip?
            .get(&(r.file_id, r.name_line, r.callee_name))
            .copied()
    }

    #[inline]
    fn file_mentions_scope(&self, file_id: u32, scope: Spur) -> bool {
        self.file_scopes
            .and_then(|sets| sets.get(file_id as usize))
            .is_some_and(|set| set.contains(&scope))
    }

    #[inline]
    fn qualifier_module_file(&self, file_id: u32, qualifier: Spur) -> Option<u32> {
        self.module_qualifiers?
            .get(file_id as usize)?
            .get(&qualifier)
            .copied()
    }

    #[inline]
    fn file_imports_file(&self, file_id: u32, target: u32) -> bool {
        self.file_imports
            .and_then(|sets| sets.get(file_id as usize))
            .is_some_and(|set| set.contains(&target))
    }

    #[inline]
    fn trait_index(&self) -> Option<&crate::resolve::TraitIndex> {
        self.trait_index
    }
}
