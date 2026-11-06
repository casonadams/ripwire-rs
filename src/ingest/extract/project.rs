//! The project-wide pass over every parsed file, run once the whole tree is parsed. Both cross-file passes read one
//! table of the classes the tree declares, so a class name declared twice is never used to type a receiver.

use super::types::FileParseResult;
use crate::domain::{Lang, SymKind};
use std::collections::{HashMap, HashSet};

/// The class names the tree declares, and the template names it specializes.
pub(super) struct Declared {
    /// Classes, structs, and interfaces the tree declares exactly once: a name declared twice could name either.
    pub(super) classes: HashSet<String>,
    /// Template names with an explicit specialization somewhere in the tree.
    pub(super) specialized: HashSet<String>,
    /// Full texts of the explicit specializations (`Box<int>`): a receiver that names one exactly keeps its text.
    pub(super) exact: HashSet<String>,
}

/// Type the receivers each file's own bindings left open, from the declarations of the whole tree.
pub fn resolve_project_types(parsed: &mut [Option<(Lang, FileParseResult)>]) {
    let declared = Declared::of(parsed);
    super::chains::resolve_pending_chains(parsed, &declared);
    super::crossfile::resolve_cross_file_types(parsed, &declared);
}

impl Declared {
    fn of(parsed: &[Option<(Lang, FileParseResult)>]) -> Self {
        let mut class_count: HashMap<&str, usize> = HashMap::new();
        let mut specialized = HashSet::new();
        let mut exact = HashSet::new();
        for def in parsed
            .iter()
            .flatten()
            .flat_map(|(_, file)| file.defs.iter())
        {
            if matches!(
                def.kind,
                SymKind::Class | SymKind::Struct | SymKind::Interface
            ) {
                *class_count.entry(def.name.as_str()).or_default() += 1;
            }
            if let Some(scope) = def.scope.as_deref()
                && let Some(base) = specialized_base(scope)
            {
                specialized.insert(base.to_string());
                exact.insert(scope.to_string());
            }
        }
        let classes = class_count
            .into_iter()
            .filter(|(_, count)| *count == 1)
            .map(|(name, _)| name.to_string())
            .collect();
        Self {
            classes,
            specialized,
            exact,
        }
    }

    /// The type a declared template text names: the specialization it spells exactly, else the template's bare name
    /// when the tree specializes no such name, else `None`.
    pub(super) fn template_target(&self, ty: &str) -> Option<String> {
        let Some((base, _)) = ty.split_once('<') else {
            return Some(ty.to_string());
        };
        if self.exact.contains(ty) {
            return Some(ty.to_string());
        }
        (!self.specialized.contains(base)).then(|| base.to_string())
    }
}

/// The template a specialization's scope names: `Box` for `Box<int>` and `ns::Box<int>`.
fn specialized_base(scope: &str) -> Option<&str> {
    let (base, _) = scope.split_once('<')?;
    base.rsplit("::").next().filter(|name| !name.is_empty())
}
