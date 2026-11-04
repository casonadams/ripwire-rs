use gix::bstr::ByteSlice;

use super::handle::short_id;
use super::revs::revision_commit;

/// The commit `git log -S NEEDLE -1` would print.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitSummary {
    pub short_id: String,
    pub date: String,
    pub subject: String,
}

/// Short id and committer date (`YYYY-MM-DD`) of the commit `rev` names; `Ok(None)` when `rev` names no commit.
pub fn commit_stamp(repo: &gix::Repository, rev: &str) -> Result<Option<(String, String)>, String> {
    let Some(commit) = revision_commit(repo, rev) else {
        return Ok(None);
    };
    let date = commit
        .time()
        .map_err(|e| format!("{rev}: {e}"))?
        .format_or_unix(gix::date::time::format::SHORT);
    Ok(Some((short_id(commit.id), date)))
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
