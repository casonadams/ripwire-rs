use super::outcome::{EdgeProv, Narrowing};
use crate::domain::{NodeId, Symbol};

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

pub(super) fn edge_prov(target_count: usize, narrowing: Narrowing, tier: Tier) -> EdgeProv {
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
