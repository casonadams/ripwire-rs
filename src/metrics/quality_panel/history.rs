use crate::domain::{IngestResult, Interner};
use crate::git::history::{
    CommitInfo, DEFAULT_CHURN_MONTHS, FileChurn, MERGE_BOMB_MAX_FILES, civil_from_days,
    compute_file_churn, compute_git_at_stamp, resolve_cutoff_timestamp,
    walk_repository_history_scoped,
};
use crate::serialize::XmlEscapeExt;

/// Historical ceiling: a file with this many commits in the window always fires, so a busy repository
/// keeps the fixed cut it always had.
pub const HISTORY_CUT_CEILING: u32 = 14;
/// Historical floor: no file fires on fewer window commits, so a quiet repository's hottest files surface
/// while a file touched once or twice never does.
pub const HISTORY_CUT_FLOOR: u32 = 3;
const SECS_PER_DAY: i64 = 86_400;

/// Per-file git churn backing the historical family, over the window that was walked.
#[derive(Debug, Clone, PartialEq)]
pub struct PanelHistory {
    /// Short HEAD stamp (`sha[+dirty]`) the churn was read at.
    pub at: String,
    /// Window start (epoch seconds); commits older than this are excluded.
    pub since: i64,
    /// Commits in the window a file needs before the historical family fires.
    pub cut: u32,
    /// What set `cut`: `p90` (90th percentile of changed files), `floor` or `ceiling`.
    pub cut_basis: &'static str,
    /// Indexed files with at least one commit in the window: the population `cut` is read from.
    pub measured: usize,
    /// Of `measured`, the files at or above `cut`.
    pub crossed: usize,
    /// Churn indexed by ingest file id.
    pub(super) churn: Vec<Option<FileChurn>>,
}

impl PanelHistory {
    /// Builds churn from commits, keeping only those at or after `since`.
    #[must_use]
    pub fn from_commits(
        ingest: &IngestResult,
        interner: &Interner,
        commits: &[CommitInfo],
        at: String,
        since: i64,
    ) -> Self {
        let window: Vec<&CommitInfo> = commits.iter().filter(|c| c.timestamp >= since).collect();
        let head = window.first().map_or(0, |c| c.timestamp);
        let by_path = compute_file_churn(window, head, MERGE_BOMB_MAX_FILES);
        let churn: Vec<Option<FileChurn>> = ingest
            .files
            .iter()
            .map(|f| by_path.get(interner.resolve(&f.path)).cloned())
            .collect();
        let (cut, cut_basis, measured, crossed) = history_cut(&churn);
        Self {
            at,
            since,
            cut,
            cut_basis,
            measured,
            crossed,
            churn,
        }
    }

    /// Walks the default churn window ending at HEAD of `root`.
    /// `Err` names why git history cannot back the historical family.
    pub fn load(root: &str, ingest: &IngestResult, interner: &Interner) -> Result<Self, String> {
        let repo = crate::git::repo::discover(root).ok_or("not a git repository")?;
        if repo.git_dir().join("shallow").exists() {
            return Err("shallow clone: history is truncated".to_string());
        }
        let since = resolve_cutoff_timestamp(None, DEFAULT_CHURN_MONTHS).unwrap_or(0);
        let commits =
            walk_repository_history_scoped(root, Some(since)).map_err(|e| e.to_string())?;
        let at = compute_git_at_stamp(root).ok_or("HEAD has no commit stamp")?;
        Ok(Self::from_commits(ingest, interner, &commits, at, since))
    }
}

/// The historical cut: the 90th percentile of window commits across changed files, clamped to
/// `[HISTORY_CUT_FLOOR, HISTORY_CUT_CEILING]`. Returns `(cut, basis, changed files, files at or above cut)`.
fn history_cut(churn: &[Option<FileChurn>]) -> (u32, &'static str, usize, usize) {
    let mut counts: Vec<u32> = churn.iter().flatten().map(|c| c.commits).collect();
    counts.sort_unstable();
    let n = counts.len();
    if n == 0 {
        return (HISTORY_CUT_FLOOR, "floor", 0, 0);
    }
    let p90 = counts[(9 * n).div_ceil(10) - 1];
    let basis = if p90 > HISTORY_CUT_CEILING {
        "ceiling"
    } else if p90 < HISTORY_CUT_FLOOR {
        "floor"
    } else {
        "p90"
    };
    let cut = p90.clamp(HISTORY_CUT_FLOOR, HISTORY_CUT_CEILING);
    (cut, basis, n, counts.iter().filter(|&&c| c >= cut).count())
}

pub(crate) fn iso_date(epoch: i64) -> String {
    let (y, m, d) = civil_from_days(epoch.div_euclid(SECS_PER_DAY));
    format!("{y:04}-{m:02}-{d:02}")
}

/// Historical root attributes. `hstatus` says whether the family is `ranked` (rows fired), `measured-empty`
/// (churn read, no function ranked) or `unavailable` (no history; `unavailable=` names why). `window`, `at`,
/// `hcut`, `hbasis` (`p90`, `floor` or `ceiling`), `hfiles` (changed files measured) and `hcrossed` (of
/// those, files at or above `hcut`) follow when history was read.
pub(crate) fn history_root_attrs(history: &Result<PanelHistory, String>, hranked: usize) -> String {
    match history {
        Err(_) => " hstatus=\"unavailable\"".to_string(),
        Ok(h) => format!(
            " window=\"12mo@{}\" at=\"{}\" hcut=\"{}\" hbasis=\"{}\" hfiles=\"{}\" hcrossed=\"{}\" hstatus=\"{}\"",
            iso_date(h.since),
            h.at.as_str().xml_escaped(),
            h.cut,
            h.cut_basis,
            h.measured,
            h.crossed,
            if hranked == 0 {
                "measured-empty"
            } else {
                "ranked"
            }
        ),
    }
}
