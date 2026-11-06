//! Nested types of the languages that scope them (`LanguageSemantics::nested_types_are_scoped`): a nested type is
//! named by the type that encloses it. `Outer.Inner` names the one declared directly in `Outer`, and a bare `Inner`
//! written inside `Outer` names it before any other type of that name.

use super::{TraitEnv, TraitIndex};
use crate::domain::NodeId;
use lasso::Spur;

impl TraitIndex {
    /// The nested type a dotted path names: the one declared directly in a type named by its penultimate segment.
    pub(super) fn qualified_nested(&self, text: &str, env: &TraitEnv<'_>) -> Option<NodeId> {
        let (outer_path, leaf_text) = text.rsplit_once('.')?;
        let outer = spur(env, last_segment(outer_path))?;
        let leaf = spur(env, leaf_text)?;
        let mut found = None;
        for &id in self.named_nominals(leaf) {
            let sym = &env.symbols[id as usize];
            if sym.scope == Some(outer) && is_scoped(sym) {
                if found.is_some() {
                    return None;
                }
                found = Some(id);
            }
        }
        found
    }

    /// The candidate nested in the innermost type enclosing byte `byte` of `file_id`, when one does.
    pub(super) fn lexically_nested(
        &self,
        admitted: &[NodeId],
        file_id: u32,
        byte: u32,
        env: &TraitEnv<'_>,
    ) -> Option<NodeId> {
        let mut best: Option<(u32, NodeId)> = None;
        for &id in admitted {
            if let Some(start) = self.enclosing_type_start(id, file_id, byte, env)
                && best.is_none_or(|(seen, _)| start > seen)
            {
                best = Some((start, id));
            }
        }
        best.map(|(_, id)| id)
    }

    fn named_nominals(&self, name: Spur) -> &[NodeId] {
        match self.nominals_by_name.get(&name) {
            Some(ids) => ids,
            None => &[],
        }
    }

    /// Start of the type that declares candidate `id` and encloses byte `byte` of `file_id`.
    fn enclosing_type_start(
        &self,
        id: NodeId,
        file_id: u32,
        byte: u32,
        env: &TraitEnv<'_>,
    ) -> Option<u32> {
        let symbols = env.symbols;
        let sym = &symbols[id as usize];
        let outer = sym.scope.filter(|_| is_scoped(sym))?;
        let mut start = None;
        for &encloser in self.named_nominals(outer) {
            let e = &symbols[encloser as usize];
            let encloses = e.id != id
                && e.file_id == file_id
                && e.file_id == sym.file_id
                && e.sig_start_byte <= byte
                && byte < e.end_byte
                && e.sig_start_byte <= sym.sig_start_byte
                && sym.sig_start_byte < e.end_byte;
            if encloses {
                start = start.max(Some(e.sig_start_byte));
            }
        }
        start
    }
}

/// Whether the language of `sym` names nested types through their enclosing type.
fn is_scoped(sym: &crate::domain::Symbol) -> bool {
    sym.lang.semantics().nested_types_are_scoped()
}

fn last_segment(path: &str) -> &str {
    path.rsplit_once('.').map_or(path, |(_, last)| last)
}

fn spur(env: &TraitEnv<'_>, text: &str) -> Option<Spur> {
    env.interner.get(text)
}
