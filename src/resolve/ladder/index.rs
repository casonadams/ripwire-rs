use crate::domain::{FileInfo, Interner, NodeId, RefRole, Reference, Symbol};
use lasso::Spur;
use std::collections::HashMap;
use std::path::Path;

/// Assign integer directory IDs based on the parent directory of each file.
#[must_use]
pub fn compute_file_dirs(files: &[FileInfo], interner: &Interner) -> Vec<u32> {
    let mut dir_map: HashMap<String, u32> = HashMap::new();
    let mut file_dirs = Vec::with_capacity(files.len());
    for f in files {
        let path = interner.resolve(&f.path);
        let dir = Path::new(path)
            .parent()
            .and_then(|p| p.to_str())
            .unwrap_or("")
            .to_string();
        let next_id = dir_map.len() as u32;
        let dir_id = *dir_map.entry(dir).or_insert(next_id);
        file_dirs.push(dir_id);
    }
    file_dirs
}

/// Index symbols by their interned name.
#[must_use]
pub fn index_symbols_by_name(symbols: &[Symbol]) -> HashMap<Spur, Vec<NodeId>> {
    let mut by_name: HashMap<Spur, Vec<NodeId>> = HashMap::new();
    for s in symbols {
        by_name.entry(s.name).or_default().push(s.id);
    }
    by_name
}

/// Index per-file referenced identifiers and qualifiers for cross-file receiver evidence.
#[must_use]
pub fn index_file_scope_mentions(
    files_len: usize,
    references: &[Reference],
) -> Vec<std::collections::HashSet<Spur>> {
    let mut sets = vec![std::collections::HashSet::new(); files_len];
    for r in references {
        if let Some(set) = sets.get_mut(r.file_id as usize) {
            set.insert(r.callee_name);
            if let Some(q) = r.qualifier {
                set.insert(q);
            }
        }
    }
    sets
}

/// Per file, the identifiers its imports bind to an indexed module file (`import x as y` binds `y`).
#[must_use]
pub fn index_module_qualifiers(
    ingest: &crate::domain::IngestResult,
    interner: &Interner,
) -> Vec<HashMap<Spur, u32>> {
    let file_index: HashMap<&str, u32> = ingest
        .files
        .iter()
        .map(|f| (interner.resolve(&f.path), f.id))
        .collect();
    let mut maps = vec![HashMap::new(); ingest.files.len()];
    for r in ingest
        .references
        .iter()
        .filter(|r| r.role == RefRole::Import)
    {
        let raw = interner.resolve(&r.callee_name);
        let Some(importer) = ingest.files.get(r.file_id as usize) else {
            continue;
        };
        let importer_path = interner.resolve(&importer.path);
        for (target, bound) in r.lang.semantics().import_module_bindings(raw) {
            let Some(bound) = interner.get(&bound) else {
                continue;
            };
            if let Some(file) = crate::languages::resolve_import(
                r.lang,
                importer_path,
                &target,
                &file_index,
                &ingest.import_aliases,
            ) {
                maps[r.file_id as usize].insert(bound, file);
            }
        }
    }
    for symbol in ingest.symbols.iter().filter(|symbol| !symbol.has_body()) {
        let Some(raw) = symbol
            .lang
            .semantics()
            .symbol_implied_import(symbol.kind, interner.resolve(&symbol.name))
        else {
            continue;
        };
        let Some(importer) = ingest.files.get(symbol.file_id as usize) else {
            continue;
        };
        if let Some(target) = crate::languages::resolve_import(
            symbol.lang,
            interner.resolve(&importer.path),
            &raw,
            &file_index,
            &ingest.import_aliases,
        ) {
            maps[symbol.file_id as usize].insert(symbol.name, target);
        }
    }
    bind_path_qualifiers(ingest, interner, &file_index, &mut maps);
    maps
}

fn bind_path_qualifiers(
    ingest: &crate::domain::IngestResult,
    interner: &Interner,
    file_index: &HashMap<&str, u32>,
    maps: &mut [HashMap<Spur, u32>],
) {
    for r in ingest
        .references
        .iter()
        .filter(|r| r.lang.semantics().keeps_path_qualifier())
    {
        let (Some(q), Some(importer)) = (r.qualifier, ingest.files.get(r.file_id as usize)) else {
            continue;
        };
        let map = &mut maps[r.file_id as usize];
        let text = interner.resolve(&q);
        if map.contains_key(&q) || !text.contains("::") {
            continue;
        }
        if let Some(file) = crate::languages::resolve_import(
            r.lang,
            interner.resolve(&importer.path),
            text,
            file_index,
            &ingest.import_aliases,
        ) {
            map.insert(q, file);
        }
    }
}

/// Per file, the indexed files its imports resolve to; resolution is the importing language's own.
#[must_use]
pub fn index_file_imports(
    ingest: &crate::domain::IngestResult,
    interner: &Interner,
) -> Vec<std::collections::HashSet<u32>> {
    let file_index: HashMap<&str, u32> = ingest
        .files
        .iter()
        .map(|f| (interner.resolve(&f.path), f.id))
        .collect();
    let mut sets = vec![std::collections::HashSet::new(); ingest.files.len()];
    for r in ingest
        .references
        .iter()
        .filter(|r| r.role == RefRole::Import)
    {
        let Some(importer) = ingest.files.get(r.file_id as usize) else {
            continue;
        };
        if let Some(target) = crate::languages::resolve_import(
            r.lang,
            interner.resolve(&importer.path),
            interner.resolve(&r.callee_name),
            &file_index,
            &ingest.import_aliases,
        ) {
            sets[r.file_id as usize].insert(target);
        }
    }
    sets
}
