use super::handle::{discover, enclosing_repo};
use super::revs::{base_of_range, merge_base_id, revision_commit, revision_tree};
use super::status::dirty_paths;
use super::tree::{is_binary, path_blobs};

/// The merge base a ref resolved to, and the paths that differ from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefChanges {
    pub base: String,
    pub paths: Vec<String>,
}

/// Paths whose content differs between the merge base of `rev` and `HEAD` and the working tree:
/// committed changes since that base plus uncommitted and untracked edits, compared by blob id,
/// with that merge base. Errors name why `rev` cannot be honored, without a flag prefix.
pub fn changes_since_ref(root: &str, rev: &str) -> Result<RefChanges, String> {
    let repo = enclosing_repo(root)?.ok_or_else(|| format!("{root} is not a git repository"))?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| format!("{root} has no work tree"))?;
    if revision_commit(&repo, rev).is_none() {
        return Err(format!("unknown ref '{rev}'"));
    }
    let base = merge_base_id(&repo, rev)?
        .ok_or_else(|| format!("'{rev}' shares no history with HEAD"))?
        .to_hex()
        .to_string();
    let base_blobs = path_blobs(&repo, &base)?;
    let head_blobs = path_blobs(&repo, "HEAD")?;
    let dirty: std::collections::BTreeSet<String> = dirty_paths(root)?.into_iter().collect();
    let worktree = dirty
        .iter()
        .map(|path| Ok((path.clone(), worktree_blob_id(&repo, workdir, path)?)))
        .collect::<Result<std::collections::BTreeMap<_, _>, String>>()?;
    let paths = changed_paths(&base_blobs, &head_blobs, &dirty, |path| {
        worktree.get(path).copied().flatten()
    });
    Ok(RefChanges { base, paths })
}

/// [`changes_since_ref`] for a `REV`, `REV..HEAD` or `REV...HEAD` spec.
pub fn changes_since_spec(root: &str, spec: &str) -> Result<RefChanges, String> {
    changes_since_ref(root, base_of_range(spec)?)
}

/// New-side changed lines of each of `paths` against its blob at `base`, read from the working
/// tree. Binary files are left out, so callers fall back to whole-file attribution for them; a
/// path that cannot be read is an error, never an empty side.
pub fn changed_lines_since(
    root: &str,
    base: &str,
    paths: &[String],
) -> Result<std::collections::BTreeMap<String, crate::gate::pr_context::ChangedLines>, String> {
    let mut out = std::collections::BTreeMap::new();
    let repo = discover(root).ok_or_else(|| format!("{root} is not a git repository"))?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| format!("{root} has no work tree"))?;
    let tree = revision_tree(&repo, base).ok_or_else(|| format!("'{base}' has no tree"))?;
    for path in paths {
        let old = match tree
            .lookup_entry_by_path(path)
            .map_err(|e| format!("{path}: {e}"))?
        {
            Some(entry) => entry
                .object()
                .map_err(|e| format!("{path}: {e}"))?
                .data
                .to_vec(),
            None => Vec::new(),
        };
        let new = match std::fs::read(workdir.join(path)) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(format!("{path}: {e}")),
        };
        if let Some(changes) = text_changes(&old, &new) {
            out.insert(path.clone(), changes);
        }
    }
    Ok(out)
}

/// Changed lines of a text file; `None` for binary content, which callers attribute as a whole file.
#[must_use]
pub fn text_changes(old: &[u8], new: &[u8]) -> Option<crate::gate::pr_context::ChangedLines> {
    (!is_binary(old) && !is_binary(new)).then(|| new_side_changes(old, new))
}

/// Changed line ranges of `new` against `old` (0-based, half-open; empty where lines were only
/// deleted), with the byte offset each line of `new` starts at.
#[must_use]
pub fn new_side_changes(old: &[u8], new: &[u8]) -> crate::gate::pr_context::ChangedLines {
    use gix::diff::blob::{Algorithm, Diff, InternedInput, sources::byte_lines};
    let input = InternedInput::new(byte_lines(old), byte_lines(new));
    let diff = Diff::compute(Algorithm::Histogram, &input);
    let line_starts = std::iter::once(0)
        .chain(
            new.iter()
                .enumerate()
                .filter(|&(i, byte)| *byte == b'\n' && i + 1 < new.len())
                .map(|(i, _)| u32::try_from(i + 1).unwrap_or(u32::MAX)),
        )
        .collect();
    crate::gate::pr_context::ChangedLines {
        ranges: diff.hunks().map(|h| h.after).collect(),
        line_starts,
    }
}

/// Paths whose working-tree blob differs from their blob at the merge base. A path outside `dirty`
/// is in the working tree exactly as at `HEAD`; `worktree` hashes the file of a dirty path, and
/// `None` means the file is gone.
#[must_use]
pub fn changed_paths(
    base: &std::collections::BTreeMap<String, gix::ObjectId>,
    head: &std::collections::BTreeMap<String, gix::ObjectId>,
    dirty: &std::collections::BTreeSet<String>,
    worktree: impl Fn(&str) -> Option<gix::ObjectId>,
) -> Vec<String> {
    let paths: std::collections::BTreeSet<&str> = base
        .keys()
        .chain(head.keys())
        .chain(dirty.iter())
        .map(String::as_str)
        .collect();
    paths
        .into_iter()
        .filter(|path| {
            let now = if dirty.contains(*path) {
                worktree(path)
            } else {
                head.get(*path).copied()
            };
            now != base.get(*path).copied()
        })
        .map(str::to_string)
        .collect()
}

/// Blob id of a working-tree file, hashed in-process; `None` when the file is absent.
fn worktree_blob_id(
    repo: &gix::Repository,
    workdir: &std::path::Path,
    path: &str,
) -> Result<Option<gix::ObjectId>, String> {
    let data = match std::fs::read(workdir.join(path)) {
        Ok(data) => data,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{path}: {e}")),
    };
    gix::objs::compute_hash(repo.object_hash(), gix::objs::Kind::Blob, &data)
        .map(Some)
        .map_err(|e| format!("{path}: {e}"))
}
