use super::ResolutionLookup;
use crate::domain::{Interner, NodeId, Reference, Symbol};
use lasso::Spur;
use std::collections::HashMap;

/// Concrete lookup context bundling symbol tables, directory mapping, and optional interner.
pub struct ResolutionContext<'a> {
    pub symbols: &'a [Symbol],
    pub file_dirs: &'a [u32],
    pub by_name: &'a HashMap<Spur, Vec<NodeId>>,
    pub interner: Option<&'a Interner>,
    pub file_scopes: Option<&'a [std::collections::HashSet<Spur>]>,
    pub local_scopes: Option<&'a crate::resolve::LocalScopes>,
    pub module_qualifiers: Option<&'a [HashMap<Spur, u32>]>,
    pub file_imports: Option<&'a [std::collections::HashSet<u32>]>,
    pub reexports: Option<&'a crate::resolve::ReexportTable>,
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

    fn bound_locally(&self, r: &Reference) -> bool {
        r.lang.semantics().local_definition_shadows_name()
            && self
                .local_scopes
                .and_then(|scopes| scopes.get(&(r.file_id, r.callee_name)))
                .is_some_and(|spans| {
                    spans
                        .iter()
                        .any(|&(start, end)| start <= r.start_byte && r.start_byte < end)
                })
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
    fn file_imports_name(&self, file_id: u32, target: u32, name: Spur) -> bool {
        self.file_imports_file(file_id, target)
            || self
                .reexports
                .is_some_and(|table| table.binds(file_id, target, name))
    }

    #[inline]
    fn trait_index(&self) -> Option<&crate::resolve::TraitIndex> {
        self.trait_index
    }

    #[inline]
    fn reexports_to(&self, module: u32, target: u32, name: Spur) -> bool {
        self.reexports
            .is_some_and(|table| table.reexports_to(module, target, name))
    }

    #[inline]
    fn imported_from_outside(&self, file_id: u32, name: Spur) -> bool {
        self.reexports
            .is_some_and(|table| table.imports_outside(file_id, name))
    }
}
