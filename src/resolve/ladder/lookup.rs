use super::candidates::{CandBuffer, filter_in_file, filter_matching_scope, filter_unscoped};
use super::definitions::{primary_template, tied_definition, yields_to_definition};
use super::outcome::{EdgeProv, Narrowing, RefOutcome, ResolvedRef};
use super::precedence::{Tier, edge_prov, is_unique_global_group, select_precedence_tier};
use super::std_filter::{
    classify_rejected_candidates, is_method_std_member_blocked, is_non_table_read_write,
    is_unpinned_std_member_blocked,
};
use crate::domain::{Interner, NO_NODE, NodeId, RefRole, Reference, SymKind, Symbol};
use lasso::Spur;
use std::collections::HashMap;

mod context;
pub use context::ResolutionContext;

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
        if r.role == RefRole::Access {
            let caller_scope = self
                .symbols()
                .get(r.from_symbol as usize)
                .and_then(|cs| cs.scope);
            let semantics = r.lang.semantics();
            let standard = self
                .interner()
                .and_then(|i| i.try_resolve(&r.callee_name))
                .is_some_and(|name| {
                    semantics.is_std_or_builtin_member(name)
                        || semantics.std_property_names().contains(&name)
                });
            let symbols = self.symbols();
            let ambiguous = self.by_name().get(&r.callee_name).is_some_and(|ids| {
                ids.iter()
                    .filter(|&&id| symbols[id as usize].accessor)
                    .count()
                    > 1
            });
            return sym.accessor
                && sym.lang.is_compatible(r.lang)
                && !((standard || ambiguous)
                    && is_method_std_member_blocked(self, r, sym, caller_scope));
        }
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

    /// The last segment of a path qualifier (`writer::MinifyingWriter` -> `MinifyingWriter`), when it
    /// is one and is interned.
    #[must_use]
    fn qualifier_leaf(&self, q: Spur) -> Option<Spur> {
        let interner = self.interner()?;
        let (_, leaf) = interner.try_resolve(&q)?.rsplit_once("::")?;
        interner.get(leaf)
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
        let mut qualified = filter_matching_scope(&cands, symbols, q);
        if qualified.is_empty()
            && let Some(leaf) = self.qualifier_leaf(q)
        {
            qualified = filter_matching_scope(&cands, symbols, leaf);
        }
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
            RefRole::Access => true,
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
        if narrowing == Narrowing::None
            && r.qualifier.is_none()
            && !r.member_call
            && let Some(imported) = self.imported_candidates(&cands, caller_file_id)
        {
            return self.resolve_targets(r, imported, Narrowing::Import, Tier::SameDir, template);
        }
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

    /// Candidates declared in files the caller imports, when none sits in the caller's own file (a local
    /// definition shadows an import) and at least one does. A caller that imports two files naming the
    /// symbol gets both, labelled as a split.
    fn imported_candidates(&self, cands: &[NodeId], caller_file: u32) -> Option<Vec<NodeId>> {
        let symbols = self.symbols();
        if cands
            .iter()
            .any(|&id| symbols[id as usize].file_id == caller_file)
        {
            return None;
        }
        let imported: Vec<NodeId> = cands
            .iter()
            .copied()
            .filter(|&id| {
                let sym = &symbols[id as usize];
                sym.scope.is_none() && self.file_imports_file(caller_file, sym.file_id)
            })
            .collect();
        (!imported.is_empty() && imported.len() < cands.len()).then_some(imported)
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
