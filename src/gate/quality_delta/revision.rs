use crate::domain::{IngestResult, Interner};
use crate::gate::baseline::{QualitySnapshot, compute_snapshot};
use crate::gate::rename::attach_body_fingerprints;
use crate::query::commands::QueryContext;
use std::path::Path;

fn remove_file_entries_from_snapshot(snap: &mut QualitySnapshot, file_prefix: &str) {
    let prefix = format!("{file_prefix}::");
    snap.ccx_by_sym.retain(|k, _| !k.starts_with(&prefix));
    snap.cx_by_sym.retain(|k, _| !k.starts_with(&prefix));
    snap.loc_by_sym.retain(|k, _| !k.starts_with(&prefix));
    snap.nest_by_sym.retain(|k, _| !k.starts_with(&prefix));
    snap.params_by_sym.retain(|k, _| !k.starts_with(&prefix));
    snap.dead_symbols.retain(|k| !k.starts_with(&prefix));
    snap.public_api.retain(|k| !k.starts_with(&prefix));
    snap.body_by_sym.retain(|k, _| !k.starts_with(&prefix));
}

fn merge_snapshot_entries(dst: &mut QualitySnapshot, src: QualitySnapshot) {
    dst.ccx_by_sym.extend(src.ccx_by_sym);
    dst.cx_by_sym.extend(src.cx_by_sym);
    dst.loc_by_sym.extend(src.loc_by_sym);
    dst.nest_by_sym.extend(src.nest_by_sym);
    dst.params_by_sym.extend(src.params_by_sym);
    dst.dead_symbols.extend(src.dead_symbols);
    dst.public_api.extend(src.public_api);
    dst.body_by_sym.extend(src.body_by_sym);
}

/// Fingerprints the working callables of the changed files: the rename pairing's "appeared" side.
pub(super) fn attach_working_bodies(
    working: &mut QualitySnapshot,
    state: &QueryContext<'_>,
    changed_files: &[String],
) {
    if !changed_files.is_empty() {
        attach_body_fingerprints(
            working,
            state.ingest,
            state.interner,
            changed_files,
            state.sources,
        );
    }
}

/// Fingerprints the HEAD callables of the changed files: the rename pairing's "vanished" side. Skipped when the
/// working side has none to pair with (`--dmm` never pairs), so HEAD blobs are not tokenized again.
fn attach_head_bodies(
    delta: &mut QualitySnapshot,
    head_ingest: &IngestResult,
    head_interner: &Interner,
    changed_files: &[String],
    working: &QualitySnapshot,
    slices: &[(&str, crate::domain::Lang, &[u8])],
) {
    if working.body_by_sym.is_empty() {
        return;
    }
    let sources: Vec<(&str, &[u8])> = slices.iter().map(|(p, _, b)| (*p, *b)).collect();
    attach_body_fingerprints(
        delta,
        head_ingest,
        head_interner,
        changed_files,
        &sources.as_slice(),
    );
}

fn head_tree<'a>(repo: &'a gix::Repository) -> Option<gix::Tree<'a>> {
    repo.head_commit().ok()?.tree().ok()
}

fn resolve_rev_tree<'a>(repo: &'a gix::Repository, rev: &str) -> Option<gix::Tree<'a>> {
    match rev {
        "" | "HEAD" => head_tree(repo),
        rev => crate::git::repo::revision_tree(repo, rev).or_else(|| head_tree(repo)),
    }
}

/// The changed paths whose baseline versions to read, in path order, each with its language: the
/// working index's when the file still exists, else its extension's (a deleted file). Paths of no
/// known language are left out.
#[must_use]
pub fn baseline_paths_to_read<'a>(
    ingest: &IngestResult,
    interner: &Interner,
    changed_set: &std::collections::HashSet<&'a str>,
) -> Vec<(&'a str, crate::domain::Lang)> {
    let indexed: std::collections::HashMap<&str, crate::domain::Lang> = ingest
        .files
        .iter()
        .map(|f| (interner.resolve(&f.path), f.lang))
        .collect();
    let mut paths: Vec<(&str, crate::domain::Lang)> = changed_set
        .iter()
        .map(|&rel_path| {
            let lang = indexed
                .get(rel_path)
                .copied()
                .unwrap_or_else(|| crate::domain::Lang::from_path(Path::new(rel_path)));
            (rel_path, lang)
        })
        .filter(|&(_, lang)| lang != crate::domain::Lang::Unknown)
        .collect();
    paths.sort_unstable_by_key(|&(path, _)| path);
    paths
}

/// The baseline blob of each changed path [`baseline_paths_to_read`] names.
fn collect_tree_blobs(
    tree: &gix::Tree<'_>,
    ingest: &IngestResult,
    interner: &Interner,
    changed_set: &std::collections::HashSet<&str>,
) -> Vec<(String, crate::domain::Lang, Vec<u8>)> {
    baseline_paths_to_read(ingest, interner, changed_set)
        .into_iter()
        .filter_map(|(rel_path, lang)| {
            let entry = tree.lookup_entry_by_path(rel_path).ok().flatten()?;
            let obj = entry.object().ok()?;
            Some((rel_path.to_string(), lang, obj.data.to_vec()))
        })
        .collect()
}

fn read_changed_rev_blobs<P: AsRef<Path>>(
    root_path: P,
    rev: &str,
    ingest: &IngestResult,
    interner: &Interner,
    changed_set: &std::collections::HashSet<&str>,
) -> Vec<(String, crate::domain::Lang, Vec<u8>)> {
    let Ok(repo) = gix::open(root_path.as_ref()) else {
        return Vec::new();
    };
    let Some(tree) = resolve_rev_tree(&repo, rev) else {
        return Vec::new();
    };
    collect_tree_blobs(&tree, ingest, interner, changed_set)
}

#[must_use]
pub fn load_revision_baseline_incremental(
    root: &str,
    rev: &str,
    ingest: &IngestResult,
    interner: &Interner,
    working: &QualitySnapshot,
    changed_files: &[String],
    head_sha: &str,
) -> QualitySnapshot {
    let changed_set: std::collections::HashSet<&str> =
        changed_files.iter().map(String::as_str).collect();
    let rev_files = read_changed_rev_blobs(root, rev, ingest, interner, &changed_set);
    let slices: Vec<(&str, crate::domain::Lang, &[u8])> = rev_files
        .iter()
        .map(|(p, l, b)| (p.as_str(), *l, b.as_slice()))
        .collect();
    let tree = WorkingTree {
        ingest,
        interner,
        snapshot: working,
        changed_files,
    };
    rebuild_baseline(&tree, &slices, head_sha)
}

/// The working index and snapshot a baseline is rebuilt from, with the paths that differ from it.
pub struct WorkingTree<'a> {
    pub ingest: &'a IngestResult,
    pub interner: &'a Interner,
    pub snapshot: &'a QualitySnapshot,
    pub changed_files: &'a [String],
}

/// The baseline: the working snapshot with each changed file replaced by its baseline version in
/// `slices`. A symbol is dead at the baseline only when it existed there and no reference in the baseline
/// versions of the changed files uses its name, so removing a function's last caller in another file makes
/// it newly dead.
#[must_use]
pub fn rebuild_baseline(
    tree: &WorkingTree<'_>,
    slices: &[(&str, crate::domain::Lang, &[u8])],
    head_sha: &str,
) -> QualitySnapshot {
    let changed_set: std::collections::HashSet<&str> =
        tree.changed_files.iter().map(String::as_str).collect();
    let mut snap = tree.snapshot.clone();
    snap.head_sha = head_sha.to_string();
    for cf in &changed_set {
        remove_file_entries_from_snapshot(&mut snap, cf);
    }
    let head_interner = crate::domain::Interner::new();
    let head_ingest = match crate::ingest::LanguageRegistry::new() {
        Ok(registry) if !slices.is_empty() => {
            crate::ingest::ingest_in_memory_files(slices, &registry, &head_interner)
        }
        _ => IngestResult::new(),
    };
    if !head_ingest.symbols.is_empty() {
        let head_graph = crate::graph::Graph::build(&head_ingest, &head_interner);
        let mut delta_snap = compute_snapshot(&head_ingest, &head_graph, &head_interner, head_sha);
        attach_head_bodies(
            &mut delta_snap,
            &head_ingest,
            &head_interner,
            tree.changed_files,
            tree.snapshot,
            slices,
        );
        merge_snapshot_entries(&mut snap, delta_snap);
    }
    // A symbol dead in the working tree is dead at the baseline when it existed there and no reference in
    // the baseline version of a changed file uses its name. References in unchanged files are the same on
    // both sides, and the working snapshot already judged them.
    let reached: std::collections::HashSet<&str> = head_ingest
        .references
        .iter()
        .map(|r| head_interner.resolve(&r.callee_name))
        .collect();
    snap.dead_symbols = tree
        .snapshot
        .dead_symbols
        .iter()
        .filter(|key| {
            let file = key.split("::").next().unwrap_or(key);
            let existed = !changed_set.contains(file) || snap.ccx_by_sym.contains_key(*key);
            existed && !reached.contains(key.rsplit("::").next().unwrap_or(key))
        })
        .cloned()
        .collect();
    snap
}

pub(super) fn load_head_baseline_incremental(
    root: &str,
    state: &QueryContext<'_>,
    working: &QualitySnapshot,
    changed_files: &[String],
    head_sha: &str,
) -> QualitySnapshot {
    load_revision_baseline_incremental(
        root,
        "HEAD",
        state.ingest,
        state.interner,
        working,
        changed_files,
        head_sha,
    )
}
