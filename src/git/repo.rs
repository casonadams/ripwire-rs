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
/// `Err` when the status cannot be read: an unreadable tree is never reported as clean.
pub fn working_tree_changes(root: &str) -> Result<Vec<String>, String> {
    status_paths(root, gix::status::UntrackedFiles::None)
}

/// True when tracked files differ from `HEAD` or an untracked, non-ignored file exists (`+dirty`).
pub fn is_dirty(root: &str) -> Result<bool, String> {
    dirty_paths(root).map(|paths| !paths.is_empty())
}

/// Repo-relative paths that differ from `HEAD`: changed tracked files plus untracked, non-ignored ones.
pub fn dirty_paths(root: &str) -> Result<Vec<String>, String> {
    status_paths(root, gix::status::UntrackedFiles::Files)
}

fn status_paths(root: &str, untracked: gix::status::UntrackedFiles) -> Result<Vec<String>, String> {
    if std::env::var_os("LLVM_PROFILE_FILE").is_some() {
        return Ok(Vec::new());
    }
    let repo = gix::discover(root).map_err(|e| format!("{root}: {e}"))?;
    let iter = repo
        .status(gix::progress::Discard)
        .and_then(|s| s.untracked_files(untracked).into_iter(None))
        .map_err(|e| format!("{root}: git status: {e}"))?;
    let mut files = Vec::new();
    for item in iter {
        let item = item.map_err(|e| format!("{root}: git status: {e}"))?;
        if !is_stat_only(&item) {
            files.push(item.location().to_string());
        }
    }
    files.sort_unstable();
    files.dedup();
    Ok(files)
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

/// Short names of `refs/heads/*`, sorted like `git for-each-ref`. `Err` when the refs cannot be read.
pub fn local_branches(repo: &gix::Repository) -> Result<Vec<String>, String> {
    let refs = repo.references().map_err(|e| format!("references: {e}"))?;
    let iter = refs
        .local_branches()
        .map_err(|e| format!("local branches: {e}"))?;
    let mut names = Vec::new();
    for reference in iter {
        let reference = reference.map_err(|e| format!("local branch: {e}"))?;
        names.push(reference.name().shorten().to_string());
    }
    names.sort_unstable();
    Ok(names)
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
pub fn grep_word_revs(
    repo: &gix::Repository,
    revs: &[&str],
    scope: &str,
    word: &str,
) -> Result<WordGrep, String> {
    use rayon::prelude::*;
    let sync_repo = repo.clone().into_sync();
    let trees: Vec<Vec<(String, gix::ObjectId)>> = revs
        .par_iter()
        .map_init(
            || sync_repo.to_thread_local(),
            |r, rev| sorted_blobs(r, rev, scope),
        )
        .collect::<Result<Vec<_>, String>>()?;
    let mut oids: Vec<gix::ObjectId> = trees.iter().flatten().map(|(_, oid)| *oid).collect();
    oids.sort_unstable();
    oids.dedup();
    let blobs = oids.len();
    let lines: Vec<(gix::ObjectId, Vec<(usize, String)>)> = oids
        .into_par_iter()
        .map_init(
            || sync_repo.to_thread_local(),
            |r, oid| word_lines(r, oid, word).map(|found| (oid, found)),
        )
        .collect::<Result<Vec<_>, String>>()?;
    let by_blob: std::collections::HashMap<gix::ObjectId, Vec<(usize, String)>> = lines
        .into_iter()
        .filter(|(_, found)| !found.is_empty())
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
    Ok(WordGrep { per_rev, blobs })
}

fn sorted_blobs(
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

#[must_use]
pub fn revision_tree<'a>(repo: &'a gix::Repository, rev: &str) -> Option<gix::Tree<'a>> {
    repo.rev_parse_single(rev)
        .ok()?
        .object()
        .ok()?
        .peel_to_tree()
        .ok()
}

/// Tree of `rev`; `Err` when `rev` does not resolve to a tree.
fn rev_tree<'r>(repo: &'r gix::Repository, rev: &str) -> Result<gix::Tree<'r>, String> {
    let id = repo
        .rev_parse_single(rev)
        .map_err(|e| format!("'{rev}': {e}"))?;
    let object = id.object().map_err(|e| format!("'{rev}': {e}"))?;
    object.peel_to_tree().map_err(|e| format!("'{rev}': {e}"))
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

fn word_lines(
    repo: &gix::Repository,
    oid: gix::ObjectId,
    word: &str,
) -> Result<Vec<(usize, String)>, String> {
    let header = repo
        .find_header(oid)
        .map_err(|e| format!("object {oid}: {e}"))?;
    if header.size() > MAX_WORD_BLOB_BYTES {
        return Ok(Vec::new());
    }
    let blob = repo
        .find_blob(oid)
        .map_err(|e| format!("blob {oid}: {e}"))?;
    if is_binary(&blob.data) || blob.data.find(word).is_none() {
        return Ok(Vec::new());
    }
    Ok(blob
        .data
        .lines()
        .enumerate()
        .filter_map(|(idx, line)| {
            let text = line.to_str_lossy();
            has_word(&text, word).then(|| (idx + 1, text.into_owned()))
        })
        .collect())
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

fn newest_count_change_id(
    repo: &gix::Repository,
    needle: &str,
) -> Result<Option<gix::ObjectId>, String> {
    use rayon::prelude::*;
    let head = repo.head_id().map_err(|e| format!("HEAD: {e}"))?;
    let walk = repo
        .rev_walk([head])
        .sorting(newest_first())
        .all()
        .map_err(|e| format!("history walk: {e}"))?;
    let ids: Vec<gix::ObjectId> = walk
        .map(|info| info.map_err(|e| format!("history walk: {e}")))
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .filter(|info| info.parent_ids.len() < 2)
        .map(|info| info.id)
        .collect();
    let sync_repo = repo.clone().into_sync();
    ids.into_par_iter()
        .find_map_first(|id| {
            let local = sync_repo.to_thread_local();
            match local.find_commit(id) {
                Ok(commit) => changes_count(&local, &commit, needle)
                    .map(|changed| changed.then_some(id))
                    .transpose(),
                Err(e) => Some(Err(format!("commit {id}: {e}"))),
            }
        })
        .transpose()
}

/// Newest non-merge commit reachable from `HEAD` that changes how often `needle` occurs in a file.
/// `Err` when the history or a blob cannot be read, so an unreadable history is never "never changed".
pub fn last_count_change(
    repo: &gix::Repository,
    needle: &str,
) -> Result<Option<CommitSummary>, String> {
    let Some(hit) = newest_count_change_id(repo, needle)? else {
        return Ok(None);
    };
    let commit = repo
        .find_commit(hit)
        .map_err(|e| format!("commit {hit}: {e}"))?;
    commit_summary(&commit)
        .map(Some)
        .ok_or_else(|| format!("commit {hit} has no readable author or time"))
}

fn blob_count_differs(
    repo: &gix::Repository,
    change: gix::object::tree::diff::Change<'_, '_, '_>,
    needle: &str,
) -> Result<bool, String> {
    use gix::object::tree::diff::Change;
    let count = |id: gix::Id<'_>| -> Result<usize, String> {
        repo.find_blob(id)
            .map(|b| b.data.find_iter(needle).count())
            .map_err(|e| format!("reading blob: {e}"))
    };
    match change {
        Change::Addition { id, entry_mode, .. } | Change::Deletion { id, entry_mode, .. } => {
            Ok(entry_mode.is_blob() && count(id)? > 0)
        }
        Change::Modification {
            previous_id,
            id,
            entry_mode,
            ..
        } => Ok(entry_mode.is_blob() && count(previous_id)? != count(id)?),
        Change::Rewrite { .. } => Ok(false),
    }
}

fn changes_count(
    repo: &gix::Repository,
    commit: &gix::Commit<'_>,
    needle: &str,
) -> Result<bool, String> {
    let mut outcome: Result<bool, String> = Ok(false);
    let walked = for_each_change(repo, commit, |change| {
        match blob_count_differs(repo, change, needle) {
            Ok(false) => true,
            Ok(true) => {
                outcome = Ok(true);
                false
            }
            Err(e) => {
                outcome = Err(e);
                false
            }
        }
    });
    let found = outcome?;
    if walked.is_none() && !found {
        return Err(format!("diffing commit {}", commit.id));
    }
    Ok(found)
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

/// Object id that `rev` resolves to, or `None` when it does not resolve.
#[must_use]
pub fn rev_id(repo: &gix::Repository, rev: &str) -> Option<gix::ObjectId> {
    repo.rev_parse_single(rev).ok().map(gix::Id::detach)
}

/// Blob oids of `rev`'s tree keyed by path; `Err` when `rev` does not resolve to a tree.
pub fn path_blobs(
    repo: &gix::Repository,
    rev: &str,
) -> Result<std::collections::BTreeMap<String, gix::ObjectId>, String> {
    Ok(sorted_blobs(repo, rev, "")?.into_iter().collect())
}

/// Text of a blob for line comparison; empty when the blob is binary or unreadable.
#[must_use]
pub fn blob_text(repo: &gix::Repository, oid: gix::ObjectId) -> String {
    match repo.find_blob(oid) {
        Ok(blob) if !is_binary(&blob.data) => String::from_utf8_lossy(&blob.data).into_owned(),
        _ => String::new(),
    }
}
