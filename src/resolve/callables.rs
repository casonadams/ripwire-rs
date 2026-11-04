use super::{RefOutcome, ResolutionLookup, TraitEnv};
use crate::domain::{IngestResult, NodeId, RefRole, Reference, SymKind, Symbol};
use lasso::Spur;
use std::collections::HashMap;

#[derive(Default)]
pub struct CallableIndex {
    by_field: HashMap<Spur, Vec<FieldTargets>>,
    ambiguous_fields: std::collections::HashSet<Spur>,
}

struct FieldTargets {
    owner: NodeId,
    targets: Vec<NodeId>,
}

impl CallableIndex {
    pub fn build(ingest: &IngestResult, lookup: &impl ResolutionLookup) -> Self {
        let mut index = Self::default();
        for binding in ingest
            .callable_bindings
            .iter()
            .filter(|binding| binding.target.is_none())
        {
            index.declare_field(binding, lookup);
        }
        for binding in ingest
            .callable_bindings
            .iter()
            .filter(|binding| binding.target.is_some())
        {
            index.assign_field(binding, lookup);
        }
        for groups in index.by_field.values_mut() {
            groups.sort_unstable_by_key(|group| group.owner);
            for group in groups {
                group.targets.sort_unstable();
                group.targets.dedup();
            }
        }
        index
    }

    fn declare_field(
        &mut self,
        binding: &crate::domain::CallableBinding,
        lookup: &impl ResolutionLookup,
    ) {
        let Some(owner) = binding_owner(binding, lookup) else {
            self.ambiguous_fields.insert(binding.field);
            return;
        };
        let groups = self.by_field.entry(binding.field).or_default();
        if !groups.iter().any(|group| group.owner == owner) {
            groups.push(FieldTargets {
                owner,
                targets: Vec::new(),
            });
        }
    }

    fn assign_field(
        &mut self,
        binding: &crate::domain::CallableBinding,
        lookup: &impl ResolutionLookup,
    ) {
        let Some(owner) = binding_owner(binding, lookup) else {
            return;
        };
        let Some(group) = self
            .by_field
            .get_mut(&binding.field)
            .and_then(|groups| groups.iter_mut().find(|group| group.owner == owner))
        else {
            return;
        };
        let Some(target) = binding.target else { return };
        let Some(lang) = lookup
            .symbols()
            .get(binding.from_symbol as usize)
            .map(|symbol| symbol.lang)
        else {
            return;
        };
        let mut reference = Reference::new(
            binding.from_symbol,
            target,
            binding.file_id,
            0,
            lang,
            RefRole::Call,
        );
        reference.qualifier = binding.target_qualifier;
        if let RefOutcome::Resolved(resolved) = lookup.classify_target(&reference) {
            group
                .targets
                .extend(resolved.targets.into_iter().filter(|&id| {
                    binding_target_matches(binding, &lookup.symbols()[id as usize], lookup)
                }));
        }
    }

    pub fn targets(
        &self,
        reference: &Reference,
        lookup: &impl ResolutionLookup,
    ) -> Option<&[NodeId]> {
        if !reference.lang.semantics().has_trait_impls() {
            return None;
        }
        let groups = self.by_field.get(&reference.callee_name)?;
        if let Some(receiver) = reference.receiver_type {
            let owner = receiver_owner(receiver, reference.file_id, reference.start_byte, lookup)?;
            return groups
                .iter()
                .find(|group| group.owner == owner)
                .map(|group| group.targets.as_slice());
        }
        if self.ambiguous_fields.contains(&reference.callee_name) {
            return None;
        }
        let [group] = groups.as_slice() else {
            return None;
        };
        Some(&group.targets)
    }
}

fn binding_owner(
    binding: &crate::domain::CallableBinding,
    lookup: &impl ResolutionLookup,
) -> Option<NodeId> {
    if let Some(symbol) = lookup.symbols().get(binding.from_symbol as usize)
        && is_type(symbol)
        && symbol.name == binding.owner
    {
        return Some(symbol.id);
    }
    receiver_owner(binding.owner, binding.file_id, binding.start_byte, lookup)
}

fn binding_target_matches(
    binding: &crate::domain::CallableBinding,
    symbol: &Symbol,
    lookup: &impl ResolutionLookup,
) -> bool {
    if !symbol.has_body()
        && lookup
            .trait_index()
            .is_some_and(|index| index.is_non_inherent(symbol.id))
    {
        return false;
    }
    symbol.is_callable()
        && binding.target_qualifier.is_none_or(|qualifier| {
            symbol.scope == Some(qualifier)
                || lookup.qualifier_module_file(binding.file_id, qualifier) == Some(symbol.file_id)
        })
}

fn is_type(symbol: &Symbol) -> bool {
    matches!(symbol.kind, SymKind::Class | SymKind::Struct)
}

fn receiver_owner(
    receiver: Spur,
    file_id: u32,
    byte: u32,
    lookup: &impl ResolutionLookup,
) -> Option<NodeId> {
    let interner = lookup.interner()?;
    let qualifier_file = |file: u32, q: Spur| lookup.qualifier_module_file(file, q);
    let reaches = |module: u32, target: u32, name: Spur| lookup.reexports_to(module, target, name);
    let env = TraitEnv {
        symbols: lookup.symbols(),
        file_dirs: lookup.file_dirs(),
        interner,
        qualifier_file: &qualifier_file,
        reaches: &reaches,
    };
    let owner = lookup.trait_index()?.resolve_receiver_owner(
        interner.resolve(&receiver),
        file_id,
        byte,
        &env,
    )?;
    is_type(&lookup.symbols()[owner as usize]).then_some(owner)
}
