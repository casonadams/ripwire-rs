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

/// The file a `from pkg import name` names when `pkg.name` is an indexed module.
fn resolve_submodule_import(
    ingest: &crate::domain::IngestResult,
    interner: &Interner,
    file_index: &HashMap<&str, u32>,
    import: &crate::domain::SubmoduleImport,
) -> Option<u32> {
    let importer = ingest.files.get(import.file_id as usize)?;
    crate::languages::resolve_import(
        importer.lang,
        interner.resolve(&importer.path),
        interner.resolve(&import.target),
        file_index,
        &ingest.import_aliases,
    )
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
        let raw = import_target_text(r, interner);
        let Some(importer) = ingest.files.get(r.file_id as usize) else {
            continue;
        };
        let importer_path = interner.resolve(&importer.path);
        for (target, bound) in r.lang.semantics().import_module_bindings(&raw) {
            let bound = interner.intern(&bound);
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
    for import in &ingest.submodule_imports {
        if let Some(file) = resolve_submodule_import(ingest, interner, &file_index, import) {
            maps[import.file_id as usize].insert(import.name, file);
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

/// The text an import reference names: its qualifier and name for languages whose import keeps the path.
fn import_target_text(r: &Reference, interner: &Interner) -> String {
    let name = interner.resolve(&r.callee_name);
    match r.qualifier {
        Some(qualifier) if r.lang.semantics().import_names_full_path() => {
            format!("{}.{name}", interner.resolve(&qualifier))
        }
        _ => name.to_string(),
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
            &import_target_text(r, interner),
            &file_index,
            &ingest.import_aliases,
        ) {
            sets[r.file_id as usize].insert(target);
        }
    }
    for import in &ingest.submodule_imports {
        if let Some(file) = resolve_submodule_import(ingest, interner, &file_index, import) {
            sets[import.file_id as usize].insert(file);
        }
    }
    sets
}

/// Per file, what its re-exporting `use` declarations bind: each bound name with the module file its path
/// names, and each glob with the module file it names. Built only for languages whose imports re-export
/// (`import_reexports`), so a path through a facade reaches the file that defines the name.
#[derive(Debug, Default)]
pub struct ReexportTable {
    named: Vec<Vec<(Spur, u32)>>,
    globs: Vec<Vec<u32>>,
    /// Per file, the names its imports bind from paths outside the index (`use serde_json::json;`).
    outside: Vec<Vec<Spur>>,
}

impl ReexportTable {
    /// True when `file` imports `name` from a path outside the indexed crate, so a call to it is external.
    #[must_use]
    pub fn imports_outside(&self, file: u32, name: Spur) -> bool {
        self.outside
            .get(file as usize)
            .is_some_and(|names| names.contains(&name))
    }
    /// True when `caller` reaches the definition of `name` in `target` through its imports: a named import
    /// of `name` or a glob, followed through the re-exports of each module it names.
    #[must_use]
    pub fn binds(&self, caller: u32, target: u32, name: Spur) -> bool {
        let mut visited = std::collections::HashSet::new();
        let named = self.named.get(caller as usize).into_iter().flatten();
        named
            .filter(|(bound, _)| *bound == name)
            .any(|&(_, module)| self.reaches(module, target, name, &mut visited))
            || self
                .globs
                .get(caller as usize)
                .into_iter()
                .flatten()
                .any(|&module| self.reaches(module, target, name, &mut visited))
    }

    fn reaches(
        &self,
        module: u32,
        target: u32,
        name: Spur,
        visited: &mut std::collections::HashSet<u32>,
    ) -> bool {
        if module == target {
            return true;
        }
        if !visited.insert(module) {
            return false;
        }
        let named = self.named.get(module as usize).into_iter().flatten();
        named
            .filter(|(bound, _)| *bound == name)
            .any(|&(_, next)| self.reaches(next, target, name, visited))
            || self
                .globs
                .get(module as usize)
                .into_iter()
                .flatten()
                .any(|&next| self.reaches(next, target, name, visited))
    }

    /// True when module `module` exposes the definition of `name` that lives in file `target`, through its
    /// own definition or the re-exports it names, transitively.
    #[must_use]
    pub fn reexports_to(&self, module: u32, target: u32, name: Spur) -> bool {
        self.reaches(module, target, name, &mut std::collections::HashSet::new())
    }
}

/// The re-export table of every file: each `use` binding and glob of a re-exporting language, resolved
/// with that language's own import rules.
#[must_use]
pub fn index_reexports(ingest: &crate::domain::IngestResult, interner: &Interner) -> ReexportTable {
    let file_index: HashMap<&str, u32> = ingest
        .files
        .iter()
        .map(|f| (interner.resolve(&f.path), f.id))
        .collect();
    let len = ingest.files.len();
    let mut table = ReexportTable {
        named: vec![Vec::new(); len],
        globs: vec![Vec::new(); len],
        outside: vec![Vec::new(); len],
    };
    for r in ingest
        .references
        .iter()
        .filter(|r| r.role == RefRole::Import && r.lang.semantics().import_reexports())
    {
        let Some(importer) = ingest.files.get(r.file_id as usize) else {
            continue;
        };
        let importer_path = interner.resolve(&importer.path);
        let resolve = |target: &str| {
            crate::languages::resolve_import(
                r.lang,
                importer_path,
                target,
                &file_index,
                &ingest.import_aliases,
            )
        };
        let raw = interner.resolve(&r.callee_name);
        let file = r.file_id as usize;
        if let Some(module) = raw.strip_suffix("::*") {
            if let Some(target) = resolve(module) {
                table.globs[file].push(target);
            }
            continue;
        }
        for (path, bound) in r.lang.semantics().import_module_bindings(raw) {
            let Some(name) = interner.get(&bound) else {
                continue;
            };
            match resolve(&path) {
                Some(target) => table.named[file].push((name, target)),
                None if is_outside_path(&path) => table.outside[file].push(name),
                None => {}
            }
        }
    }
    table
}

/// A Rust path that begins with neither an anchor (`crate`, `self`, `super`) nor a module of the index: it
/// names an external crate or the standard library.
fn is_outside_path(path: &str) -> bool {
    let head = path.split("::").next().unwrap_or_default();
    !head.is_empty() && !matches!(head, "crate" | "self" | "super")
}
