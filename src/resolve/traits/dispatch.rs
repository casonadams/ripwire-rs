use super::TraitImpl;
use super::TraitIndex;
use super::paths::type_leaf;
use crate::domain::{NodeId, Symbol};
use lasso::Spur;
use std::collections::HashMap;

impl TraitIndex {
    /// Methods declared in a trait body or a trait impl block: never inherent methods of a type.
    #[must_use]
    pub fn is_non_inherent(&self, method: NodeId) -> bool {
        self.non_inherent.contains(&method)
    }

    /// Methods named `name` that trait `trait_id` declares in its own body (bodied or bodyless).
    #[must_use]
    pub fn declared_methods(
        &self,
        trait_id: NodeId,
        name: Spur,
        symbols: &[Symbol],
    ) -> Vec<NodeId> {
        named_members(&self.trait_members, trait_id, name, symbols)
    }

    /// Concrete subject types with an impl of `trait_id`.
    #[must_use]
    pub fn implementor_ids(&self, trait_id: NodeId) -> Vec<NodeId> {
        let mut out: Vec<NodeId> = self
            .by_trait
            .get(&trait_id)
            .into_iter()
            .flatten()
            .filter_map(|&i| self.impls[i].subject)
            .collect();
        out.sort_unstable();
        out.dedup();
        out
    }

    /// Methods named `name` a call through trait `trait_id` may reach: each impl's override, or
    /// the trait default where the impl does not override.
    #[must_use]
    pub fn dispatch(&self, trait_id: NodeId, name: Spur, symbols: &[Symbol]) -> Vec<NodeId> {
        let idxs = self
            .by_trait
            .get(&trait_id)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut out = Vec::new();
        if idxs.is_empty() {
            out.extend(self.default_methods(trait_id, name, symbols));
        }
        for &i in idxs {
            out.extend(self.impl_targets(&self.impls[i], name, symbols));
        }
        sort_dedup(out)
    }

    #[must_use]
    pub fn subject_targets(&self, subject: NodeId, name: Spur, symbols: &[Symbol]) -> Vec<NodeId> {
        let own = self.own_subject_targets(subject, name, symbols);
        if !own.is_empty() {
            return own;
        }
        self.ancestors(subject)
            .into_iter()
            .map(|ancestor| self.own_subject_targets(ancestor, name, symbols))
            .find(|found| !found.is_empty())
            .unwrap_or_default()
    }

    /// Targets `name` in every ancestor of `subject`: the overloads a subclass inherits beside its own.
    #[must_use]
    pub fn inherited_subject_targets(
        &self,
        subject: NodeId,
        name: Spur,
        symbols: &[Symbol],
    ) -> Vec<NodeId> {
        let mut out = Vec::new();
        for ancestor in self.ancestors(subject) {
            out.extend(self.own_subject_targets(ancestor, name, symbols));
        }
        sort_dedup(out)
    }

    /// Bodyless declarations named `name` that a subject writes in its own body, or else its nearest
    /// ancestor that writes one: a C-family prototype or pure virtual, whose definition may be elsewhere.
    #[must_use]
    pub fn subject_declarations(
        &self,
        subject: NodeId,
        name: Spur,
        symbols: &[Symbol],
    ) -> Vec<NodeId> {
        std::iter::once(subject)
            .chain(self.ancestors(subject))
            .find(|&class| {
                !self.own_subject_targets(class, name, symbols).is_empty()
                    || !self
                        .own_subject_declarations(class, name, symbols)
                        .is_empty()
            })
            .map_or_else(Vec::new, |class| {
                self.own_subject_declarations(class, name, symbols)
            })
    }

    fn own_subject_declarations(
        &self,
        subject: NodeId,
        name: Spur,
        symbols: &[Symbol],
    ) -> Vec<NodeId> {
        let members = self
            .by_subject
            .get(&subject)
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let out: Vec<NodeId> = members
            .iter()
            .flat_map(|&i| self.impls[i].methods.iter().copied())
            .filter(|&m| {
                let sym = &symbols[m as usize];
                sym.name == name
                    && crate::resolve::is_declaration(sym)
                    && !sym.lang.semantics().declarations_yield_to_definitions()
            })
            .collect();
        sort_dedup(out)
    }

    fn own_subject_targets(&self, subject: NodeId, name: Spur, symbols: &[Symbol]) -> Vec<NodeId> {
        let mut out = Vec::new();
        for &i in self
            .by_subject
            .get(&subject)
            .map(Vec::as_slice)
            .unwrap_or(&[])
        {
            out.extend(self.impl_targets(&self.impls[i], name, symbols));
        }
        for &i in &self.blankets {
            let imp = &self.impls[i];
            if imp
                .bound
                .is_some_and(|bound| self.implements(subject, bound))
            {
                out.extend(self.impl_targets(imp, name, symbols));
            }
        }
        sort_dedup(out)
    }

    fn implements(&self, subject: NodeId, trait_id: NodeId) -> bool {
        self.by_subject.get(&subject).is_some_and(|idxs| {
            idxs.iter()
                .any(|&i| self.impls[i].trait_id == Some(trait_id))
        })
    }

    fn impl_targets(&self, imp: &TraitImpl, name: Spur, symbols: &[Symbol]) -> Vec<NodeId> {
        let own: Vec<NodeId> = imp
            .methods
            .iter()
            .copied()
            .filter(|&m| named_with_body(symbols, m, name))
            .collect();
        match (own.is_empty(), imp.trait_id) {
            (true, Some(trait_id)) => self.default_methods(trait_id, name, symbols),
            _ => own,
        }
    }

    fn default_methods(&self, trait_id: NodeId, name: Spur, symbols: &[Symbol]) -> Vec<NodeId> {
        named_members(&self.trait_defaults, trait_id, name, symbols)
    }

    #[must_use]
    pub fn ancestors(&self, class: NodeId) -> Vec<NodeId> {
        const MAX_ANCESTORS: usize = 32;
        let mut out: Vec<NodeId> = Vec::new();
        let mut next = 0;
        let mut frontier = vec![class];
        while next < frontier.len() && out.len() < MAX_ANCESTORS {
            let current = frontier[next];
            next += 1;
            for &base in self.supertypes.get(&current).map_or(&[][..], Vec::as_slice) {
                if base != class && !out.contains(&base) {
                    out.push(base);
                    frontier.push(base);
                }
            }
        }
        out
    }

    #[must_use]
    pub fn external_subject_targets(
        &self,
        receiver: &str,
        name: Spur,
        symbols: &[Symbol],
    ) -> Vec<NodeId> {
        let idxs = self
            .by_external_subject
            .get(type_leaf(receiver))
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let mut out = Vec::new();
        for &i in idxs {
            out.extend(self.impl_targets(&self.impls[i], name, symbols));
        }
        sort_dedup(out)
    }
}

fn named_with_body(symbols: &[Symbol], id: NodeId, name: Spur) -> bool {
    let s = &symbols[id as usize];
    s.name == name && s.has_body()
}

fn named_members(
    members: &HashMap<NodeId, Vec<NodeId>>,
    owner: NodeId,
    name: Spur,
    symbols: &[Symbol],
) -> Vec<NodeId> {
    members
        .get(&owner)
        .map(|ids| {
            ids.iter()
                .copied()
                .filter(|&m| symbols[m as usize].name == name)
                .collect()
        })
        .unwrap_or_default()
}

fn sort_dedup(mut ids: Vec<NodeId>) -> Vec<NodeId> {
    ids.sort_unstable();
    ids.dedup();
    ids
}
