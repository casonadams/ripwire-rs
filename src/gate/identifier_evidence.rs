use crate::domain::{IngestResult, Interner, Lang};
use crate::gate::baseline::NameScope;
use crate::graph::Graph;
use crate::query::SourceReader;
use std::collections::{HashMap, HashSet};

/// Which of the dead-code `rows` (each a file path and the symbol key `path::[Scope::]name`) some identifier
/// elsewhere still uses: a macro argument, a table entry, a value or a re-export reaches a function without a
/// call edge. Where a language scopes bare names, a use only counts when it can mean this definition: in
/// its own file, or in one that neither defines nor binds the name itself, imports another definition of
/// it, nor takes it from an outside package.
pub fn identifier_used_rows(
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
    sources: &dyn SourceReader,
    rows: &[(&str, &str)],
) -> Vec<bool> {
    let file_of: HashMap<&str, (Lang, u32)> = ingest
        .files
        .iter()
        .map(|f| (interner.resolve(&f.path), (f.lang, f.id)))
        .collect();
    let mut names: HashMap<Lang, HashSet<&str>> = HashMap::new();
    for &(path, key) in rows {
        if let Some(&(lang, _)) = file_of.get(path) {
            names
                .entry(lang)
                .or_default()
                .insert(key.rsplit("::").next().unwrap_or(key));
        }
    }
    let uses: HashMap<Lang, HashMap<String, Vec<(u32, u32)>>> = names
        .iter()
        .map(|(&lang, list)| {
            let found = crate::ingest::identifiers::identifier_use_sites(
                ingest, interner, sources, lang, list,
            );
            (lang, found)
        })
        .collect();
    let scope = NameScope::of(ingest, graph, interner);
    rows.iter()
        .map(|&(path, key)| {
            let Some(&(lang, file)) = file_of.get(path) else {
                return false;
            };
            let name = key.rsplit("::").next().unwrap_or(key);
            let Some(users) = uses.get(&lang).and_then(|found| found.get(name)) else {
                return false;
            };
            if !lang.semantics().local_definition_shadows_name() {
                return true;
            }
            let Some(spur) = interner.get(name) else {
                return true;
            };
            let method = key.matches("::").count() > 1;
            users.iter().any(|&(user, byte)| {
                scope.reaches_at(user, file, spur, byte)
                    && !(!method && scope.is_excused(user, spur))
            })
        })
        .collect()
}
