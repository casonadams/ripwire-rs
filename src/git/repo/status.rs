use gix::bstr::ByteSlice;
use gix::status::plumbing::index_as_worktree::EntryStatus;

use super::handle::{discover, workdir_prefix};

pub fn for_each_tracked_path(root: &str, mut visit: impl FnMut(&str)) {
    let Some(repo) = discover(root) else { return };
    let Ok(scope) = workdir_prefix(&repo, root) else {
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
