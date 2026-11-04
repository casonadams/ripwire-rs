use std::collections::{BinaryHeap, HashMap, HashSet};
use std::fmt;
use std::path::Path;

pub const CHURN_DECAY_HALF_LIFE_DAYS: f64 = 90.0;
pub const MERGE_BOMB_MAX_FILES: usize = 100;
pub const DEFAULT_CHURN_MONTHS: u32 = 12;
pub const DEFAULT_COCHANGE_MONTHS: u32 = 18;

/// A parsed git commit with its timestamp and changed files.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommitInfo {
    pub id: String,
    pub timestamp: i64,
    pub author: String,
    pub changed_files: Vec<String>,
}

/// The time or revision boundary for git mining.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SinceScope {
    AllHistory,
    SinceDate(i64),
    SinceRevision(String),
}

/// Churn metrics computed for a single file.
#[derive(Debug, Clone, PartialEq)]
pub struct FileChurn {
    pub file_path: String,
    pub commits: u32,
    pub decayed_weight: f64,
    pub last_commit_epoch: i64,
}

#[derive(Debug)]
pub enum GitHistoryError {
    Open(String),
    Head(String),
    Walk(String),
    TreeDiff(String),
}

impl fmt::Display for GitHistoryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open(msg) => write!(f, "failed to open repository: {msg}"),
            Self::Head(msg) => write!(f, "failed to resolve HEAD: {msg}"),
            Self::Walk(msg) => write!(f, "failed walking revisions: {msg}"),
            Self::TreeDiff(msg) => write!(f, "failed diffing trees: {msg}"),
        }
    }
}

impl std::error::Error for GitHistoryError {}

/// Convert a civil calendar date (Y, M, D) to days since Unix epoch (1970-01-01).
#[must_use]
pub fn days_from_civil(mut y: i64, m: u32, d: u32) -> i64 {
    if m <= 2 {
        y -= 1;
    }
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = (y - era * 400) as u32;
    let m_adj = if m > 2 { m - 3 } else { m + 9 };
    let doy = (153 * m_adj + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + (doe as i64) - 719468
}

/// Convert days since Unix epoch to civil date (year, month, day).
#[must_use]
pub fn civil_from_days(mut z: i64) -> (i64, u32, u32) {
    z += 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = (yoe as i64) + era * 400 + if m <= 2 { 1 } else { 0 };
    (y, m, d)
}

/// Calculate the timestamp `months` calendar months before `epoch`.
#[must_use]
pub fn months_before_epoch(epoch: i64, months: u32) -> i64 {
    let days = if epoch >= 0 {
        epoch / 86400
    } else {
        -(-epoch + 86399) / 86400
    };
    let sec_of_day = epoch - days * 86400;
    let (y, m, d) = civil_from_days(days);
    let month_idx = y * 12 + (m as i64) - 1 - (months as i64);
    let ny = if month_idx >= 0 {
        month_idx / 12
    } else {
        -(-month_idx + 11) / 12
    };
    let nm = (month_idx - ny * 12 + 1) as u32;
    (days_from_civil(ny, nm, 1) + (d as i64) - 1) * 86400 + sec_of_day
}

fn parse_numeric_prefix(s: &str) -> Option<(u32, &str)> {
    let trimmed = s.trim_start();
    let num_end = trimmed
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(trimmed.len());
    if num_end == 0 {
        return None;
    }
    let n = trimmed[..num_end].parse::<u32>().ok()?;
    Some((n, trimmed[num_end..].trim()))
}

fn parse_relative_date(s: &str, anchor: i64) -> Option<i64> {
    let lower = s.to_ascii_lowercase();
    if lower == "today" {
        return Some(anchor);
    }
    if lower == "yesterday" {
        return Some(anchor - 86400);
    }
    let (n, rest) = parse_numeric_prefix(&lower)?;
    let unit = rest.strip_suffix("ago").unwrap_or(rest).trim();
    if unit.starts_with("month") {
        Some(months_before_epoch(anchor, n))
    } else if unit.starts_with("week") {
        Some(anchor - (n as i64) * 7 * 86400)
    } else if unit.starts_with("day") {
        Some(anchor - (n as i64) * 86400)
    } else if unit.starts_with("year") {
        Some(months_before_epoch(anchor, n * 12))
    } else {
        None
    }
}

fn parse_iso_date(s: &str) -> Option<i64> {
    let parts: Vec<&str> = s.split(['-', 'T', 'Z', ':']).collect();
    if parts.len() < 3 {
        return None;
    }
    let y = parts[0].parse::<i64>().ok()?;
    let m = parts[1].parse::<u32>().ok()?;
    let d = parts[2].parse::<u32>().ok()?;
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let days = days_from_civil(y, m, d);
    let mut secs = days * 86400;
    if parts.len() >= 6 {
        let hour = parts[3].parse::<i64>().ok()?;
        let min = parts[4].parse::<i64>().ok()?;
        let sec = parts[5].parse::<i64>().ok()?;
        secs += hour * 3600 + min * 60 + sec;
    }
    Some(secs)
}

fn is_commit_sha(s: &str) -> bool {
    (s.len() == 40 || s.len() == 64) && s.chars().all(|c| c.is_ascii_hexdigit())
}

/// Parse a `--since` string into a `SinceScope`.
#[must_use]
pub fn parse_since_scope(since_str: Option<&str>, anchor_epoch: i64) -> SinceScope {
    let s = match since_str {
        Some(v) if !v.trim().is_empty() => v.trim(),
        _ => return SinceScope::AllHistory,
    };
    if is_commit_sha(s) || s.starts_with("HEAD~") || s.starts_with('v') {
        return SinceScope::SinceRevision(s.to_string());
    }
    if let Some(epoch) = parse_relative_date(s, anchor_epoch) {
        return SinceScope::SinceDate(epoch);
    }
    if let Some(epoch) = parse_iso_date(s) {
        return SinceScope::SinceDate(epoch);
    }
    SinceScope::SinceRevision(s.to_string())
}

/// Resolve an optional since parameter and default month window to an epoch timestamp cutoff.
#[must_use]
pub fn resolve_cutoff_timestamp(since: Option<&str>, default_months: u32) -> Option<i64> {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    match parse_since_scope(since, now) {
        SinceScope::SinceDate(ts) => Some(ts),
        SinceScope::AllHistory => {
            if since.is_some() {
                None
            } else {
                Some(months_before_epoch(now, default_months))
            }
        }
        SinceScope::SinceRevision(_) => None,
    }
}

/// Filter commits according to the specified `SinceScope`.
#[must_use]
pub fn filter_commits_by_scope<'a>(
    commits: &'a [CommitInfo],
    scope: &SinceScope,
) -> Vec<&'a CommitInfo> {
    match scope {
        SinceScope::AllHistory => commits.iter().collect(),
        SinceScope::SinceDate(cutoff) => {
            commits.iter().filter(|c| c.timestamp >= *cutoff).collect()
        }
        SinceScope::SinceRevision(rev) => {
            let mut out = Vec::new();
            for c in commits {
                if c.id == *rev || c.id.starts_with(rev) {
                    break;
                }
                out.push(c);
            }
            out
        }
    }
}

/// Compute raw commit churn, time-decayed churn weight, and newest commit epoch per file.
#[must_use]
pub fn compute_file_churn<'a, I>(
    commits: I,
    head_epoch: i64,
    max_files_filter: usize,
) -> HashMap<String, FileChurn>
where
    I: IntoIterator<Item = &'a CommitInfo>,
{
    let mut churn_map: HashMap<String, FileChurn> = HashMap::new();
    for commit in commits {
        if commit.changed_files.len() > max_files_filter {
            continue;
        }
        let age_sec = (head_epoch - commit.timestamp).max(0);
        let age_days = (age_sec as f64) / 86400.0;
        let weight = 0.5f64.powf(age_days / CHURN_DECAY_HALF_LIFE_DAYS);

        for file in &commit.changed_files {
            let entry = churn_map.entry(file.clone()).or_insert_with(|| FileChurn {
                file_path: file.clone(),
                commits: 0,
                decayed_weight: 0.0,
                last_commit_epoch: 0,
            });
            entry.commits += 1;
            entry.decayed_weight += weight;
            if commit.timestamp > entry.last_commit_epoch {
                entry.last_commit_epoch = commit.timestamp;
            }
        }
    }
    churn_map
}

/// Per-file commit counts over `commits` (merge bombs excluded): the churn `--hotspots` ranks by.
#[must_use]
pub fn file_commit_counts(commits: &[CommitInfo]) -> HashMap<String, u32> {
    let anchor = commits.first().map_or(0, |c| c.timestamp);
    compute_file_churn(commits, anchor, MERGE_BOMB_MAX_FILES)
        .into_iter()
        .map(|(path, fc)| (path, fc.commits))
        .collect()
}

/// What the history walk needs to know about one commit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkNode {
    pub timestamp: i64,
    /// Whether the commit has two or more parents; merges are never reported.
    pub is_merge: bool,
    /// The parents the walk continues into from this commit.
    pub followed_parents: Vec<gix::ObjectId>,
}

/// A non-merge commit reached by the walk, with its commit time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawCommitMeta {
    pub id: gix::ObjectId,
    pub timestamp: i64,
}

fn walk_err(e: impl fmt::Display) -> GitHistoryError {
    GitHistoryError::Walk(e.to_string())
}

/// Resolve `id` from the object store, or `None` when the object is absent (e.g. beyond a shallow
/// boundary).
fn walk_node(
    repo: &gix::Repository,
    prefix: &str,
    id: gix::ObjectId,
) -> Result<Option<WalkNode>, GitHistoryError> {
    let Some(object) = repo.try_find_object(id).map_err(walk_err)? else {
        return Ok(None);
    };
    let commit = object.try_into_commit().map_err(walk_err)?;
    let timestamp = commit.time().map_err(walk_err)?.seconds;
    Ok(Some(WalkNode {
        timestamp,
        is_merge: commit.parent_ids().nth(1).is_some(),
        followed_parents: followed_parents(repo, &commit, prefix)?,
    }))
}

/// The tree id `commit` holds at `prefix`; `Ok(None)` when the prefix is absent from that commit.
fn tree_at_prefix(
    commit: &gix::Commit<'_>,
    prefix: &str,
) -> Result<Option<gix::ObjectId>, GitHistoryError> {
    let tree = commit.tree().map_err(walk_err)?;
    if prefix.is_empty() {
        return Ok(Some(tree.id));
    }
    let entry = tree
        .lookup_entry_by_path(prefix.trim_end_matches('/'))
        .map_err(walk_err)?;
    Ok(entry.map(|entry| entry.object_id()))
}

/// Parents to follow from `commit`: a merge identical under `prefix` to one parent follows only that
/// parent, as git's default history simplification does for a `-- <prefix>` pathspec. A parent
/// absent from the object store (a shallow boundary) cannot be compared and is not followed as a
/// match; any other read failure is an error.
fn followed_parents(
    repo: &gix::Repository,
    commit: &gix::Commit<'_>,
    prefix: &str,
) -> Result<Vec<gix::ObjectId>, GitHistoryError> {
    let parents: Vec<gix::ObjectId> = commit.parent_ids().map(gix::Id::detach).collect();
    if parents.len() < 2 {
        return Ok(parents);
    }
    let own = tree_at_prefix(commit, prefix)?;
    for &parent in &parents {
        let Some(object) = repo.try_find_object(parent).map_err(walk_err)? else {
            continue;
        };
        let parent_commit = object.try_into_commit().map_err(walk_err)?;
        if tree_at_prefix(&parent_commit, prefix)? == own {
            return Ok(vec![parent]);
        }
    }
    Ok(parents)
}

/// A commit waiting to be visited: newest first, larger id first on equal times.
struct Queued {
    id: gix::ObjectId,
    node: WalkNode,
}

impl Queued {
    fn key(&self) -> (i64, gix::ObjectId) {
        (self.node.timestamp, self.id)
    }
}

impl PartialEq for Queued {
    fn eq(&self, other: &Self) -> bool {
        self.key() == other.key()
    }
}

impl Eq for Queued {}

impl PartialOrd for Queued {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Queued {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.key().cmp(&other.key())
    }
}

/// Look up each unseen id once and queue it when the store holds it; absent objects are skipped.
fn enqueue<E>(
    ids: impl IntoIterator<Item = gix::ObjectId>,
    lookup: &mut impl FnMut(gix::ObjectId) -> Result<Option<WalkNode>, E>,
    seen: &mut HashSet<gix::ObjectId>,
    queue: &mut BinaryHeap<Queued>,
) -> Result<(), E> {
    for id in ids {
        if seen.insert(id)
            && let Some(node) = lookup(id)?
        {
            queue.push(Queued { id, node });
        }
    }
    Ok(())
}

/// Walk history from `head`, newest first, and return the non-merge commits at or after
/// `cutoff_timestamp` (inclusive). `lookup` resolves a commit id: `Ok(None)` skips that id, `Err`
/// aborts the walk. An absent `head` yields no commits.
pub fn walk_metas<E>(
    head: gix::ObjectId,
    mut lookup: impl FnMut(gix::ObjectId) -> Result<Option<WalkNode>, E>,
    cutoff_timestamp: Option<i64>,
) -> Result<Vec<RawCommitMeta>, E> {
    let mut metas = Vec::new();
    let mut seen = HashSet::new();
    let mut queue = BinaryHeap::new();
    enqueue(std::iter::once(head), &mut lookup, &mut seen, &mut queue)?;
    while let Some(Queued { id, node }) = queue.pop() {
        if cutoff_timestamp.is_some_and(|cutoff| node.timestamp < cutoff) {
            break;
        }
        if !node.is_merge {
            metas.push(RawCommitMeta {
                id,
                timestamp: node.timestamp,
            });
        }
        enqueue(node.followed_parents, &mut lookup, &mut seen, &mut queue)?;
    }
    Ok(metas)
}

/// Git-backed `walk_metas`: `head` must exist, otherwise the walk fails.
fn collect_commit_metas(
    repo: &gix::Repository,
    head: gix::ObjectId,
    prefix: &str,
    cutoff_timestamp: Option<i64>,
) -> Result<Vec<RawCommitMeta>, GitHistoryError> {
    repo.find_commit(head).map_err(walk_err)?;
    walk_metas(head, |id| walk_node(repo, prefix, id), cutoff_timestamp)
}

fn diff_commit_tree(
    repo: &gix::Repository,
    meta: &RawCommitMeta,
) -> Result<CommitInfo, GitHistoryError> {
    let commit = repo
        .find_commit(meta.id)
        .map_err(|e| GitHistoryError::Walk(e.to_string()))?;
    let mut changed_files = Vec::new();
    super::repo::for_each_change(repo, &commit, |change| {
        if !change.entry_mode().is_tree() {
            changed_files.push(change.location().to_string());
        }
        true
    })
    .ok_or_else(|| GitHistoryError::TreeDiff(meta.id.to_string()))?;
    let author = commit.author().map_err(walk_err)?.email.to_string();
    Ok(CommitInfo {
        id: meta.id.to_string(),
        timestamp: meta.timestamp,
        author,
        changed_files,
    })
}

fn process_commits_parallel(
    repo: &gix::Repository,
    metas: Vec<RawCommitMeta>,
) -> Result<Vec<CommitInfo>, GitHistoryError> {
    use rayon::prelude::*;
    let sync_repo = repo.clone().into_sync();
    metas
        .into_par_iter()
        .map(|meta| {
            let thread_repo = sync_repo.to_thread_local();
            diff_commit_tree(&thread_repo, &meta)
        })
        .collect()
}

fn strip_subdir_prefix(mut commits: Vec<CommitInfo>, prefix: &str) -> Vec<CommitInfo> {
    if prefix.is_empty() {
        return commits;
    }
    for c in &mut commits {
        c.changed_files = c
            .changed_files
            .drain(..)
            .filter_map(|f| f.strip_prefix(prefix).map(String::from))
            .collect();
    }
    commits.retain(|c| !c.changed_files.is_empty());
    commits
}

pub fn walk_repository_history<P: AsRef<Path>>(
    repo_path: P,
) -> Result<Vec<CommitInfo>, GitHistoryError> {
    walk_repository_history_scoped(repo_path.as_ref(), None)
}

/// Non-merge commits reachable from `HEAD`, newest first, newer than an optional cutoff timestamp,
/// with changed paths relative to `repo_path` (`git log --no-merges --no-renames --name-only -- .`).
pub fn walk_repository_history_scoped<P: AsRef<Path>>(
    repo_path: P,
    cutoff_timestamp: Option<i64>,
) -> Result<Vec<CommitInfo>, GitHistoryError> {
    let repo_path = repo_path.as_ref();
    let repo = gix::discover(repo_path).map_err(|e| GitHistoryError::Open(e.to_string()))?;
    let root = repo_path.to_str().ok_or_else(|| {
        GitHistoryError::Open(format!("{} is not valid UTF-8", repo_path.display()))
    })?;
    let prefix = super::repo::workdir_prefix(&repo, root).map_err(GitHistoryError::Open)?;
    let head = repo
        .head_id()
        .map_err(|e| GitHistoryError::Head(e.to_string()))?
        .detach();
    let metas = collect_commit_metas(&repo, head, &prefix, cutoff_timestamp)?;
    let commits = process_commits_parallel(&repo, metas)?;
    Ok(strip_subdir_prefix(commits, &prefix))
}

/// Full hex id of the repository's HEAD commit. `Ok(None)` outside a git repository or on an unborn HEAD;
/// `Err` when the repository or HEAD cannot be read.
pub fn resolve_head_sha(root: &str) -> Result<Option<String>, String> {
    let Some(repo) = super::repo::enclosing_repo(root)? else {
        return Ok(None);
    };
    Ok(super::repo::head_commit_id(&repo)?.map(|id| id.to_string()))
}

/// Compute the canonical git stamp `sha[+dirty]` for a repository root. `Ok(None)` when there is no
/// HEAD commit; `Err` when the working tree cannot be read, so its dirtiness is unknown.
pub fn compute_git_at_stamp(root: &str) -> Result<Option<String>, String> {
    let Some(full_id) = resolve_head_sha(root)? else {
        return Ok(None);
    };
    let short_sha = &full_id[..9.min(full_id.len())];
    let dirty = super::repo::is_dirty(root)?;
    Ok(Some(if dirty {
        format!("{short_sha}+dirty")
    } else {
        short_sha.to_string()
    }))
}
