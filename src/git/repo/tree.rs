use super::revs::rev_tree;

const BINARY_PROBE_LEN: usize = 8000;

pub(super) fn sorted_blobs(
    repo: &gix::Repository,
    rev: &str,
    scope: &str,
) -> Result<Vec<(String, gix::ObjectId)>, String> {
    let Some(tree) = scoped_tree(repo, rev, scope)? else {
        return Ok(Vec::new());
    };
    let mut recorder = gix::traverse::tree::Recorder::default();
    tree.traverse()
        .breadthfirst(&mut recorder)
        .map_err(|e| format!("'{rev}': tree walk: {e}"))?;
    let mut blobs: Vec<(String, gix::ObjectId)> = recorder
        .records
        .into_iter()
        .filter(|e| e.mode.is_blob())
        .map(|e| (e.filepath.to_string(), e.oid))
        .collect();
    blobs.sort_unstable();
    Ok(blobs)
}

/// The tree at `scope` (`dir/`, empty for the whole tree) in `rev`; `Ok(None)` when `scope` is
/// absent or is not a directory at `rev`.
fn scoped_tree<'r>(
    repo: &'r gix::Repository,
    rev: &str,
    scope: &str,
) -> Result<Option<gix::Tree<'r>>, String> {
    let tree = rev_tree(repo, rev)?;
    let scope = scope.trim_end_matches('/');
    if scope.is_empty() {
        return Ok(Some(tree));
    }
    let Some(object) = object_at(&tree, scope).map_err(|e| format!("'{rev}': {e}"))? else {
        return Ok(None);
    };
    Ok(object.try_into_tree().ok())
}

/// The entry at `path` in `tree`; `Ok(None)` when the path is absent from it.
pub fn entry_at<'r>(
    tree: &gix::Tree<'r>,
    path: impl AsRef<std::path::Path>,
) -> Result<Option<gix::object::tree::Entry<'r>>, String> {
    let shown = path.as_ref().display().to_string();
    tree.lookup_entry_by_path(path)
        .map_err(|e| format!("{shown}: {e}"))
}

/// The object at `path` in `tree`; `Ok(None)` when the path is absent from it.
pub fn object_at<'r>(
    tree: &gix::Tree<'r>,
    path: impl AsRef<std::path::Path>,
) -> Result<Option<gix::Object<'r>>, String> {
    let shown = path.as_ref().display().to_string();
    match entry_at(tree, path)? {
        Some(entry) => entry
            .object()
            .map(Some)
            .map_err(|e| format!("{shown}: {e}")),
        None => Ok(None),
    }
}

pub(super) fn is_binary(data: &[u8]) -> bool {
    data[..data.len().min(BINARY_PROBE_LEN)].contains(&0)
}

/// Blob oids of `rev`'s tree keyed by path; `Err` when `rev` does not resolve to a tree.
pub fn path_blobs(
    repo: &gix::Repository,
    rev: &str,
) -> Result<std::collections::BTreeMap<String, gix::ObjectId>, String> {
    Ok(sorted_blobs(repo, rev, "")?.into_iter().collect())
}

/// Text of a blob for line comparison: `Ok(None)` when the blob is binary, `Err` when it cannot be read.
pub fn blob_text(repo: &gix::Repository, oid: gix::ObjectId) -> Result<Option<String>, String> {
    let blob = repo
        .find_blob(oid)
        .map_err(|e| format!("blob {oid}: {e}"))?;
    Ok(blob_text_of(&blob.data))
}

/// Text of blob bytes for line comparison; `None` when they are binary.
#[must_use]
pub fn blob_text_of(data: &[u8]) -> Option<String> {
    (!is_binary(data)).then(|| String::from_utf8_lossy(data).into_owned())
}
