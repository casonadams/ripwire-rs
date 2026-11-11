use super::model::{QualityDeltaFacts, QualityRegression};
use super::source_checks::is_changed_callable;
use crate::gate::baseline::symbol_canonical_key;
use crate::gate::rename::RenameMap;
use crate::query::commands::QueryContext;
use std::collections::HashSet;

/// How a clone member stands against the baseline.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemberStatus {
    /// Absent at the baseline.
    New,
    /// Present at the baseline with a different body.
    Touched,
    Untouched,
}

/// A clone member's name as rows print it: `path::scope::name` for a scoped symbol, else its name.
fn member_name(sym: &crate::domain::Symbol, state: &QueryContext<'_>) -> String {
    use crate::domain::CorpusView;
    let name = state.sym_name(sym);
    match sym.scope {
        Some(sc) => {
            symbol_canonical_key(name, state.sym_path(sym), Some(state.interner.resolve(&sc)))
        }
        None => name.to_string(),
    }
}

/// Identifiers a symbol's definition uses, from its parse tree; keywords are not identifier nodes.
fn member_identifiers(sym: &crate::domain::Symbol, state: &QueryContext<'_>) -> HashSet<String> {
    use crate::domain::CorpusView;
    let mut names = HashSet::new();
    let path = state.sym_path(sym);
    let Ok(bytes) = state.sources.read(path) else {
        return names;
    };
    let Some(tree) = crate::ingest::identifiers::parse_file_tree(sym.lang, path, &bytes) else {
        return names;
    };
    if let Some(node) = tree
        .root_node()
        .descendant_for_byte_range(sym.sig_start_byte as usize, sym.end_byte as usize)
    {
        sym.lang
            .semantics()
            .for_each_identifier(node, &bytes, &mut |text, _| {
                names.insert(text.to_string());
            });
    }
    names
}

/// An idiom collision rather than a copy, as upstream reads it: every member spells one recognized
/// shape, the group is under 80 tokens, the members sit in distinct enclosing contexts, and no two
/// share an identifier.
fn is_idiom_collision(
    cg: &crate::clones::CloneGroup,
    members: &[&crate::domain::Symbol],
    state: &QueryContext<'_>,
) -> bool {
    if cg.idiom.as_str().is_none() || cg.token_count >= 80 {
        return false;
    }
    let contexts: HashSet<(u32, Option<lasso::Spur>)> =
        members.iter().map(|s| (s.file_id, s.scope)).collect();
    if contexts.len() < members.len() {
        return false;
    }
    let names: Vec<HashSet<String>> = members
        .iter()
        .map(|s| member_identifiers(s, state))
        .collect();
    names
        .iter()
        .enumerate()
        .all(|(i, a)| names[i + 1..].iter().all(|b| a.is_disjoint(b)))
}

/// Duplication rows for the clone groups a change made: a group with no new or touched member
/// already existed. A group is `new-symbol` only when every member is new; otherwise it gates.
/// A group that copies an untouched helper with two or more callers adds a
/// `new-clone-of-reused-helper` row whose `now=` is that helper's caller count.
pub fn collect_changed_clone_regressions(
    clones: &[crate::clones::CloneGroup],
    state: &QueryContext<'_>,
    status: &dyn Fn(&crate::domain::Symbol) -> MemberStatus,
    regressions: &mut Vec<QualityRegression>,
) {
    use crate::domain::CorpusView;
    for cg in clones {
        let mut members: Vec<(String, &crate::domain::Symbol, MemberStatus)> = cg
            .members
            .iter()
            .filter_map(|&id| state.ingest.symbols.get(id as usize))
            .map(|sym| (member_name(sym, state), sym, status(sym)))
            .collect();
        if members.iter().all(|m| m.2 == MemberStatus::Untouched) {
            continue;
        }
        members.sort_by(|a, b| a.0.cmp(&b.0));
        let all_new = members.iter().all(|m| m.2 == MemberStatus::New);
        let syms: Vec<&crate::domain::Symbol> = members.iter().map(|m| m.1).collect();
        let collision = !all_new && is_idiom_collision(cg, &syms, state);
        let first = members[0].1;
        let row = QualityRegression {
            kind: "duplication".to_string(),
            file_path: state.sym_path(first).to_string(),
            line: first.line,
            now_val: cg.token_count as u32,
            is_gating: !all_new && !collision,
            is_minor: collision,
            is_new_symbol: all_new,
            members: members.iter().map(|m| m.0.clone()).collect(),
            idiom: cg.idiom.as_str(),
            ..QualityRegression::default()
        };
        let reused_callers = members
            .iter()
            .filter(|m| m.2 == MemberStatus::Untouched)
            .map(|m| state.graph.in_degree(m.1.id))
            .max()
            .filter(|&callers| callers >= 2);
        if let Some(callers) = reused_callers {
            regressions.push(QualityRegression {
                kind: "new-clone-of-reused-helper".to_string(),
                now_val: callers as u32,
                is_gating: !collision,
                is_new_symbol: false,
                ..row.clone()
            });
        }
        regressions.push(row);
    }
}

/// Whether `sym`'s body differs from the baseline's. Body fingerprints exist only for a baseline rebuilt
/// from git; a pinned sidecar keeps just complexity and size, so without both fingerprints a symbol
/// counts as changed only when its cognitive complexity or length moved.
fn body_changed(
    sym: &crate::domain::Symbol,
    key: &str,
    was: &str,
    facts: &QualityDeltaFacts<'_>,
) -> bool {
    if let Some((now, then)) = facts
        .working
        .body_by_sym
        .get(key)
        .zip(facts.baseline.body_by_sym.get(was))
    {
        return !now.same_text(then);
    }
    let ccx = facts.baseline.ccx_by_sym.get(was).copied();
    let loc = facts.baseline.loc_by_sym.get(was).copied();
    ccx != Some(sym.ccx) || loc.is_some_and(|loc| loc != sym.loc)
}

/// A clone member's standing: new when the baseline lacks it, touched when its body in a changed
/// file differs from the baseline's.
pub(super) fn member_status(
    sym: &crate::domain::Symbol,
    state: &QueryContext<'_>,
    facts: &QualityDeltaFacts<'_>,
    renames: &RenameMap,
) -> MemberStatus {
    use crate::domain::CorpusView;
    let path = state.sym_path(sym);
    let scope = sym.scope.map(|sc| state.interner.resolve(&sc));
    let key = symbol_canonical_key(state.sym_name(sym), path, scope);
    let was = renames.was_key(&key);
    if !facts.baseline.ccx_by_sym.contains_key(was) {
        MemberStatus::New
    } else if is_changed_callable(sym, path, facts.changed_files)
        && body_changed(sym, &key, was, facts)
    {
        MemberStatus::Touched
    } else {
        MemberStatus::Untouched
    }
}
