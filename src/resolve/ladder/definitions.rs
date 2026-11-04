use crate::domain::{Interner, NodeId, Symbol};

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
pub(super) fn yields_to_definition(decl: &Symbol, cands: &[NodeId], symbols: &[Symbol]) -> bool {
    decl.lang.semantics().declarations_yield_to_definitions()
        && is_declaration(decl)
        && cands
            .iter()
            .any(|&id| symbols[id as usize].has_body() && symbols[id as usize].name == decl.name)
}

/// The one candidate whose scope every other candidate's scope specializes, if any.
pub(super) fn primary_template(
    defs: &[NodeId],
    symbols: &[Symbol],
    interner: &Interner,
) -> Option<NodeId> {
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
