use super::{TraitEnv, TraitIndex};
use crate::domain::{NodeId, SymKind};
use crate::resolve::scopes::Scope;
use lasso::Spur;

/// Which nominal kinds a path may name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum Want {
    Any,
    Trait,
    Concrete,
}

impl Want {
    fn admits(self, kind: SymKind) -> bool {
        match self {
            Self::Any => true,
            Self::Trait => kind == SymKind::Interface,
            Self::Concrete => kind != SymKind::Interface,
        }
    }
}

impl TraitIndex {
    /// The type a receiver names (concrete or trait) as seen from `file_id`, or `None` when
    /// the name cannot be proven to denote one indexed type.
    #[must_use]
    pub fn resolve_receiver_owner(
        &self,
        receiver: &str,
        file_id: u32,
        byte: u32,
        env: &TraitEnv<'_>,
    ) -> Option<NodeId> {
        self.resolve_path(receiver, file_id, byte, env, Want::Any)
    }

    pub(super) fn resolve_text(
        &self,
        text: Spur,
        file_id: u32,
        byte: u32,
        env: &TraitEnv<'_>,
        want: Want,
    ) -> Option<NodeId> {
        self.resolve_path(env.interner.try_resolve(&text)?, file_id, byte, env, want)
    }

    pub(super) fn resolve_path(
        &self,
        text: &str,
        file_id: u32,
        byte: u32,
        env: &TraitEnv<'_>,
        want: Want,
    ) -> Option<NodeId> {
        let (quals, leaf) = text.rsplit_once("::").unwrap_or(("", text));
        let leaf = env.interner.get(leaf)?;
        let symbols = env.symbols;
        let admitted = self
            .nominals_by_name
            .get(&leaf)?
            .iter()
            .copied()
            .filter(|&id| want.admits(symbols[id as usize].kind));
        let site = self.scopes.at(file_id, byte, symbols);
        if !quals.is_empty() {
            let scope = self
                .scopes
                .qualify(quals, site, self.crate_roots[file_id as usize], env)
                .or_else(|| self.qualified_file(quals, file_id, env))?;
            return only(admitted.filter(|&id| {
                let sym = &symbols[id as usize];
                let defined_at = self.scopes.at(sym.file_id, sym.sig_start_byte, symbols);
                // A file-root path (`crate::m::Name`) also names what that file re-exports under `Name`.
                defined_at == scope
                    || (scope.module.is_none()
                        && sym.file_id != scope.file_id
                        && (env.reaches)(scope.file_id, sym.file_id, leaf))
            }));
        }
        let admitted: Vec<NodeId> = admitted.collect();
        let mut scope = Some(site);
        while let Some(current) = scope {
            let found = only_or_ambiguous(admitted.iter().copied().filter(|&id| {
                let symbol = &symbols[id as usize];
                self.scopes
                    .at(symbol.file_id, symbol.sig_start_byte, symbols)
                    == current
            }));
            match found {
                Match::One(id) => return Some(id),
                Match::Ambiguous => return None,
                Match::None => scope = self.scopes.parent(current),
            }
        }
        self.visible_unique(&admitted, file_id, env)
    }

    fn qualified_file(&self, qualifier: &str, file_id: u32, env: &TraitEnv<'_>) -> Option<Scope> {
        let qualifier = env.interner.get(qualifier)?;
        let target = (env.qualifier_file)(file_id, qualifier)?;
        Some(Scope {
            file_id: target,
            module: None,
        })
    }

    fn visible_unique(
        &self,
        admitted: &[NodeId],
        file_id: u32,
        env: &TraitEnv<'_>,
    ) -> Option<NodeId> {
        let symbols = env.symbols;
        let in_file = admitted
            .iter()
            .copied()
            .filter(|&id| symbols[id as usize].file_id == file_id);
        match only_or_ambiguous(in_file) {
            Match::One(id) => return Some(id),
            Match::Ambiguous => return None,
            Match::None => {}
        }
        let name = symbols[*admitted.first()? as usize].name;
        if let Some(imported) = (env.qualifier_file)(file_id, name) {
            let from_import = admitted
                .iter()
                .copied()
                .filter(|&id| (env.reaches)(imported, symbols[id as usize].file_id, name));
            if let Match::One(id) = only_or_ambiguous(from_import) {
                return Some(id);
            }
        }
        let dir = env.file_dirs.get(file_id as usize).copied()?;
        let in_dir = admitted.iter().copied().filter(|&id| {
            env.file_dirs
                .get(symbols[id as usize].file_id as usize)
                .copied()
                == Some(dir)
        });
        match only_or_ambiguous(in_dir) {
            Match::One(id) => Some(id),
            Match::Ambiguous => None,
            Match::None => only(admitted.iter().copied()),
        }
    }
}

pub(super) fn type_leaf(text: &str) -> &str {
    let path = text
        .trim_start_matches(['&', ' '])
        .trim_start_matches("mut ");
    let path = path.split('<').next().unwrap_or(path).trim();
    path.rsplit("::").next().unwrap_or(path)
}

enum Match {
    None,
    One(NodeId),
    Ambiguous,
}

fn only_or_ambiguous(ids: impl IntoIterator<Item = NodeId>) -> Match {
    let mut it = ids.into_iter();
    match (it.next(), it.next()) {
        (None, _) => Match::None,
        (Some(id), None) => Match::One(id),
        _ => Match::Ambiguous,
    }
}

pub(super) fn only(ids: impl IntoIterator<Item = NodeId>) -> Option<NodeId> {
    let mut it = ids.into_iter();
    let first = it.next()?;
    it.next().is_none().then_some(first)
}
