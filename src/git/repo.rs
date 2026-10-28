use gix::bstr::ByteSlice;
use gix::status::plumbing::index_as_worktree::EntryStatus;

const SHORT_ID_LEN: usize = 9;
const BINARY_PROBE_LEN: usize = 8000;
const MAX_WORD_BLOB_BYTES: u64 = 2 * 1024 * 1024;

/// One line of a blob that contains a searched string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineHit {
    pub path: String,
    pub line: usize,
    pub text: String,
    pub blob: gix::ObjectId,
}

/// The commit `git log -S NEEDLE -1` would print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSummary {
    pub short_id: String,
    pub date: String,
    pub subject: String,
}

#[must_use]
pub fn discover(root: &str) -> Option<gix::Repository> {
    gix::discover(root).ok()
}

#[must_use]
pub fn short_id(id: impl AsRef<gix::oid>) -> String {
    id.as_ref().to_hex_with_len(SHORT_ID_LEN).to_string()
}

/// `root`'s path inside the work tree as `dir/` (empty at the top), like `git rev-parse --show-prefix`.
#[must_use]
pub fn workdir_prefix(repo: &gix::Repository, root: &str) -> Option<String> {
    let workdir = std::fs::canonicalize(repo.workdir()?).ok()?;
    let root = std::fs::canonicalize(root).ok()?;
    relative_prefix(&workdir, &root)
}

/// `root` relative to `workdir` as `dir/` (empty at the top); `None` when `root` is outside `workdir`.
#[must_use]
pub fn relative_prefix(workdir: &std::path::Path, root: &std::path::Path) -> Option<String> {
    let rel = root
        .strip_prefix(workdir)
        .ok()?
        .to_str()?
        .replace('\\', "/");
    Some(if rel.is_empty() {
        rel
    } else {
        format!("{rel}/")
    })
}

pub fn for_each_tracked_path(root: &str, mut visit: impl FnMut(&str)) {
    let Some(repo) = discover(root) else { return };
    let Some(scope) = workdir_prefix(&repo, root) else {
        return;
    };
    let Ok(index) = repo.index() else { return };
    for path in index
        .entries()
        .iter()
        .filter_map(|entry| entry.path(&index).to_str().ok())
        .filter_map(|path| path.strip_prefix(scope.as_str()))
    {
        visit(path);
    }
}

/// Paths of tracked files whose working-tree content differs from `HEAD` (`git diff --name-only HEAD`).
#[must_use]
pub fn working_tree_changes(root: &str) -> Vec<String> {
    status_paths(root, gix::status::UntrackedFiles::None)
}

/// True when tracked files differ from `HEAD` or an untracked, non-ignored file exists (`+dirty`).
#[must_use]
pub fn is_dirty(root: &str) -> bool {
    !dirty_paths(root).is_empty()
}

/// Repo-relative paths that differ from `HEAD`: changed tracked files plus untracked, non-ignored ones.
#[must_use]
pub fn dirty_paths(root: &str) -> Vec<String> {
    status_paths(root, gix::status::UntrackedFiles::Files)
}

fn status_paths(root: &str, untracked: gix::status::UntrackedFiles) -> Vec<String> {
    if std::env::var_os("LLVM_PROFILE_FILE").is_some() {
        return Vec::new();
    }
    let Some(repo) = discover(root) else {
        return Vec::new();
    };
    let Ok(iter) = repo
        .status(gix::progress::Discard)
        .and_then(|s| s.untracked_files(untracked).into_iter(None))
    else {
        return Vec::new();
    };
    let mut files: Vec<String> = iter
        .filter_map(Result::ok)
        .filter(|item| !is_stat_only(item))
        .map(|item| item.location().to_string())
        .collect();
    files.sort_unstable();
    files.dedup();
    files
}

fn is_stat_only(item: &gix::status::Item) -> bool {
    matches!(
        item,
        gix::status::Item::IndexWorktree(gix::status::index_worktree::Item::Modification {
            status: EntryStatus::NeedsUpdate(_),
            ..
        })
    )
}

/// Short names of `refs/heads/*`, sorted like `git for-each-ref`.
#[must_use]
pub fn local_branches(repo: &gix::Repository) -> Vec<String> {
    let Ok(refs) = repo.references() else {
        return Vec::new();
    };
    let Ok(iter) = refs.local_branches() else {
        return Vec::new();
    };
    let mut names: Vec<String> = iter
        .filter_map(Result::ok)
        .map(|r| r.name().shorten().to_string())
        .collect();
    names.sort_unstable();
    names
}

/// Checked-out branch short name, or `HEAD` when detached (`git rev-parse --abbrev-ref HEAD`).
#[must_use]
pub fn head_ref(repo: &gix::Repository) -> String {
    repo.head_name()
        .ok()
        .flatten()
        .map_or_else(|| "HEAD".to_string(), |n| n.shorten().to_string())
}

#[derive(Debug)]
pub struct WordGrep {
    pub per_rev: Vec<Vec<LineHit>>,
    pub blobs: usize,
}

/// Match `git grep -n -w -F` within a `dir/` scope; paths are scope-relative.
/// Shared revision blobs are read once.
#[must_use]
pub fn grep_word_revs(repo: &gix::Repository, revs: &[&str], scope: &str, word: &str) -> WordGrep {
    use rayon::prelude::*;
    let sync_repo = repo.clone().into_sync();
    let trees: Vec<Vec<(String, gix::ObjectId)>> = revs
        .par_iter()
        .map_init(
            || sync_repo.to_thread_local(),
            |r, rev| sorted_blobs(r, rev, scope),
        )
        .collect();
    let mut oids: Vec<gix::ObjectId> = trees.iter().flatten().map(|(_, oid)| *oid).collect();
    oids.sort_unstable();
    oids.dedup();
    let blobs = oids.len();
    let by_blob: std::collections::HashMap<gix::ObjectId, Vec<(usize, String)>> = oids
        .into_par_iter()
        .map_init(
            || sync_repo.to_thread_local(),
            |r, oid| (oid, word_lines(r, oid, word)),
        )
        .filter(|(_, lines)| !lines.is_empty())
        .collect();
    let per_rev = trees
        .into_iter()
        .map(|tree| {
            tree.into_iter()
                .flat_map(|(path, oid)| {
                    by_blob
                        .get(&oid)
                        .into_iter()
                        .flatten()
                        .map(move |(line, text)| LineHit {
                            path: path.clone(),
                            line: *line,
                            text: text.clone(),
                            blob: oid,
                        })
                })
                .collect()
        })
        .collect();
    WordGrep { per_rev, blobs }
}

fn sorted_blobs(repo: &gix::Repository, rev: &str, scope: &str) -> Vec<(String, gix::ObjectId)> {
    let Some(tree) = scoped_tree(repo, rev, scope) else {
        return Vec::new();
    };
    let mut recorder = gix::traverse::tree::Recorder::default();
    if tree.traverse().breadthfirst(&mut recorder).is_err() {
        return Vec::new();
    }
    let mut blobs: Vec<(String, gix::ObjectId)> = recorder
        .records
        .into_iter()
        .filter(|e| e.mode.is_blob())
        .map(|e| (e.filepath.to_string(), e.oid))
        .collect();
    blobs.sort_unstable();
    blobs
}

#[must_use]
pub fn revision_tree<'a>(repo: &'a gix::Repository, rev: &str) -> Option<gix::Tree<'a>> {
    repo.rev_parse_single(rev)
        .ok()?
        .object()
        .ok()?
        .peel_to_tree()
        .ok()
}

fn scoped_path_tree<'r>(tree: gix::Tree<'r>, scope: &str) -> Option<gix::Tree<'r>> {
    let scope = scope.trim_end_matches('/');
    if scope.is_empty() {
        return Some(tree);
    }
    tree.lookup_entry_by_path(scope)
        .ok()
        .flatten()?
        .object()
        .ok()?
        .try_into_tree()
        .ok()
}

fn scoped_tree<'r>(repo: &'r gix::Repository, rev: &str, scope: &str) -> Option<gix::Tree<'r>> {
    revision_tree(repo, rev).and_then(|tree| scoped_path_tree(tree, scope))
}

fn word_lines(repo: &gix::Repository, oid: gix::ObjectId, word: &str) -> Vec<(usize, String)> {
    if repo
        .find_header(oid)
        .is_ok_and(|header| header.size() > MAX_WORD_BLOB_BYTES)
    {
        return Vec::new();
    }
    let Ok(blob) = repo.find_blob(oid) else {
        return Vec::new();
    };
    if is_binary(&blob.data) || blob.data.find(word).is_none() {
        return Vec::new();
    }
    blob.data
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let text = line.to_str_lossy();
            has_word(&text, word).then(|| (idx + 1, text.into_owned()))
        })
        .collect()
}

/// True when `word` occurs in `line` bounded by non-identifier characters.
#[must_use]
pub fn has_word(line: &str, word: &str) -> bool {
    let is_ident = |b: u8| b.is_ascii_alphanumeric() || b == b'_';
    let bytes = line.as_bytes();
    !word.is_empty()
        && line.match_indices(word).any(|(i, _)| {
            let end = i + word.len();
            (i == 0 || !is_ident(bytes[i - 1])) && (end == bytes.len() || !is_ident(bytes[end]))
        })
}

fn is_binary(data: &[u8]) -> bool {
    data[..data.len().min(BINARY_PROBE_LEN)].contains(&0)
}

pub fn revision_commit<'a>(repo: &'a gix::Repository, rev: &str) -> Option<gix::Commit<'a>> {
    repo.rev_parse_single(rev)
        .ok()?
        .object()
        .ok()?
        .peel_to_commit()
        .ok()
}

/// Short id and committer date (`YYYY-MM-DD`) of the commit `rev` names.
#[must_use]
pub fn commit_stamp(repo: &gix::Repository, rev: &str) -> Option<(String, String)> {
    let commit = revision_commit(repo, rev)?;
    let date = commit
        .time()
        .ok()?
        .format_or_unix(gix::date::time::format::SHORT);
    Some((short_id(commit.id), date))
}

fn commit_summary(commit: &gix::Commit<'_>) -> Option<CommitSummary> {
    let date = commit
        .author()
        .ok()?
        .time()
        .ok()?
        .format_or_unix(gix::date::time::format::SHORT);
    let message = commit.message_raw_sloppy().to_str_lossy();
    Some(CommitSummary {
        short_id: short_id(commit.id),
        date,
        subject: message.lines().next().unwrap_or("").to_string(),
    })
}

fn newest_count_change_id(repo: &gix::Repository, needle: &str) -> Option<gix::ObjectId> {
    use rayon::prelude::*;
    let head = repo.head_id().ok()?;
    let ids: Vec<gix::ObjectId> = repo
        .rev_walk([head])
        .sorting(newest_first())
        .all()
        .ok()?
        .filter_map(Result::ok)
        .filter(|info| info.parent_ids.len() < 2)
        .map(|info| info.id)
        .collect();
    let sync_repo = repo.clone().into_sync();
    ids.into_par_iter().find_first(|&id| {
        let local = sync_repo.to_thread_local();
        local
            .find_commit(id)
            .is_ok_and(|commit| changes_count(&local, &commit, needle))
    })
}

/// Newest non-merge commit reachable from `HEAD` that changes how often `needle` occurs in a file.
#[must_use]
pub fn last_count_change(repo: &gix::Repository, needle: &str) -> Option<CommitSummary> {
    let hit = newest_count_change_id(repo, needle)?;
    let commit = repo.find_commit(hit).ok()?;
    commit_summary(&commit)
}

fn changes_count(repo: &gix::Repository, commit: &gix::Commit<'_>, needle: &str) -> bool {
    let count = |id: gix::Id<'_>| {
        repo.find_blob(id)
            .map_or(0, |b| b.data.find_iter(needle).count())
    };
    let mut found = false;
    let _ = for_each_change(repo, commit, |change| {
        use gix::object::tree::diff::Change;
        let differs = match change {
            Change::Addition { id, entry_mode, .. } | Change::Deletion { id, entry_mode, .. } => {
                entry_mode.is_blob() && count(id) > 0
            }
            Change::Modification {
                previous_id,
                id,
                entry_mode,
                ..
            } => entry_mode.is_blob() && count(previous_id) != count(id),
            Change::Rewrite { .. } => false,
        };
        found = differs;
        !differs
    });
    found
}

/// Visit each file change of `commit` against its first parent (or the empty tree), without rename
/// tracking; stop when `visit` returns `false`.
pub fn for_each_change<'r>(
    repo: &'r gix::Repository,
    commit: &gix::Commit<'r>,
    mut visit: impl FnMut(gix::object::tree::diff::Change<'_, '_, '_>) -> bool,
) -> Option<()> {
    let tree = commit.tree().ok()?;
    let parent_tree = parent_tree(repo, commit)?;
    let mut changes = parent_tree.changes().ok()?;
    changes.options(|o| {
        o.track_rewrites(None);
    });
    changes
        .for_each_to_obtain_tree(&tree, |change| {
            Ok::<_, gix::Error>(continue_if(visit(change)))
        })
        .ok()
        .map(|_| ())
}

/// The tree `commit` is diffed against: its first parent's tree, or the empty tree for a root commit.
fn parent_tree<'r>(repo: &'r gix::Repository, commit: &gix::Commit<'r>) -> Option<gix::Tree<'r>> {
    match commit.parent_ids().next() {
        Some(pid) => pid.object().ok()?.peel_to_tree().ok(),
        None => Some(repo.empty_tree()),
    }
}

/// Map a visitor's keep-going flag to the diff traversal's control flow.
fn continue_if(keep_going: bool) -> std::ops::ControlFlow<()> {
    if keep_going {
        std::ops::ControlFlow::Continue(())
    } else {
        std::ops::ControlFlow::Break(())
    }
}

#[must_use]
pub fn newest_first() -> gix::revision::walk::Sorting {
    gix::revision::walk::Sorting::ByCommitTime(Default::default())
}

fn branch_merge_base(
    repo: &gix::Repository,
    branch: &str,
) -> Option<(gix::ObjectId, gix::ObjectId)> {
    let tip = revision_commit(repo, branch)?.id;
    let head = repo.head_commit().ok()?.id;
    let base = repo.merge_base(head, tip).ok().flatten()?.detach();
    Some((base, tip))
}

/// Merge base of `HEAD` and `branch`, or `None` when they share no history.
#[must_use]
pub fn merge_base_id(repo: &gix::Repository, branch: &str) -> Option<gix::ObjectId> {
    branch_merge_base(repo, branch).map(|(base, _)| base)
}

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
    let repo = discover(root).ok_or_else(|| format!("{root} is not a git repository"))?;
    let workdir = repo
        .workdir()
        .ok_or_else(|| format!("{root} has no work tree"))?;
    if revision_commit(&repo, rev).is_none() {
        return Err(format!("unknown ref '{rev}'"));
    }
    let base = merge_base_id(&repo, rev)
        .ok_or_else(|| format!("'{rev}' shares no history with HEAD"))?
        .to_hex()
        .to_string();
    let base_blobs = path_blobs(&repo, &base);
    let head_blobs = path_blobs(&repo, "HEAD");
    let dirty: std::collections::BTreeSet<String> = dirty_paths(root).into_iter().collect();
    let paths = changed_paths(&base_blobs, &head_blobs, &dirty, |path| {
        worktree_blob_id(&repo, workdir, path)
    });
    Ok(RefChanges { base, paths })
}

/// [`changes_since_ref`] for a `REV`, `REV..HEAD` or `REV...HEAD` spec.
pub fn changes_since_spec(root: &str, spec: &str) -> Result<RefChanges, String> {
    changes_since_ref(root, base_of_range(spec)?)
}

/// The base of a `REV`, `REV..HEAD` or `REV...HEAD` spec. The compared side is always the working
/// tree, so a range whose right side is not `HEAD` is refused rather than silently reinterpreted.
pub fn base_of_range(spec: &str) -> Result<&str, String> {
    let Some((base, target)) = spec.split_once("..") else {
        return Ok(spec);
    };
    let target = target.strip_prefix('.').unwrap_or(target);
    if !target.is_empty() && target != "HEAD" {
        return Err(format!(
            "'{spec}': only REV or REV..HEAD is supported; the compared side is the working tree"
        ));
    }
    if base.is_empty() {
        return Err(format!("'{spec}': missing base ref"));
    }
    Ok(base)
}

/// New-side changed lines of each of `paths` against its blob at `base`, read from the working
/// tree. Binary files are left out, so callers fall back to whole-file attribution for them.
#[must_use]
pub fn changed_lines_since(
    root: &str,
    base: &str,
    paths: &[String],
) -> std::collections::BTreeMap<String, crate::gate::pr_context::ChangedLines> {
    let mut out = std::collections::BTreeMap::new();
    let Some(repo) = discover(root) else {
        return out;
    };
    let (Some(workdir), Some(tree)) = (repo.workdir(), revision_tree(&repo, base)) else {
        return out;
    };
    for path in paths {
        let old = tree
            .lookup_entry_by_path(path)
            .ok()
            .flatten()
            .and_then(|entry| entry.object().ok())
            .map(|obj| obj.data.to_vec())
            .unwrap_or_default();
        let new = std::fs::read(workdir.join(path)).unwrap_or_default();
        if is_binary(&old) || is_binary(&new) {
            continue;
        }
        out.insert(path.clone(), new_side_changes(&old, &new));
    }
    out
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
) -> Option<gix::ObjectId> {
    let data = std::fs::read(workdir.join(path)).ok()?;
    gix::objs::compute_hash(repo.object_hash(), gix::objs::Kind::Blob, &data).ok()
}

/// Object id that `rev` resolves to, or `None` when it does not resolve.
#[must_use]
pub fn rev_id(repo: &gix::Repository, rev: &str) -> Option<gix::ObjectId> {
    repo.rev_parse_single(rev).ok().map(gix::Id::detach)
}

/// Blob oids of `rev`'s tree keyed by path; empty when `rev` does not resolve.
#[must_use]
pub fn path_blobs(
    repo: &gix::Repository,
    rev: &str,
) -> std::collections::BTreeMap<String, gix::ObjectId> {
    sorted_blobs(repo, rev, "").into_iter().collect()
}

/// Text of a blob for line comparison; empty when the blob is binary or unreadable.
#[must_use]
pub fn blob_text(repo: &gix::Repository, oid: gix::ObjectId) -> String {
    match repo.find_blob(oid) {
        Ok(blob) if !is_binary(&blob.data) => String::from_utf8_lossy(&blob.data).into_owned(),
        _ => String::new(),
    }
}
