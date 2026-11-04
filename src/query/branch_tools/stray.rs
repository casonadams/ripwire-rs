use crate::query::stray_lines::{branch_verdict, classify_file};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashSet};

/// Per-branch stray report: merge-base relation, verdict, and per-file evidence sorted by stray.
fn branch_stray_report(
    repo: &gix::Repository,
    branch: &str,
    head_blobs: &BTreeMap<String, gix::ObjectId>,
    blobs: &mut HashSet<gix::ObjectId>,
) -> Result<Option<Value>, String> {
    let Some(base) = crate::git::repo::merge_base_id(repo, branch)? else {
        return Ok(None);
    };
    let base_blobs = crate::git::repo::path_blobs(repo, &base.to_hex().to_string())?;
    let tip_blobs = crate::git::repo::path_blobs(repo, branch)?;
    let mut files = Vec::new();
    for (path, &tip_oid) in &tip_blobs {
        let base_oid = base_blobs.get(path).copied();
        if base_oid == Some(tip_oid) {
            continue;
        }
        let head_oid = head_blobs.get(path).copied();
        blobs.extend([Some(tip_oid), base_oid, head_oid].into_iter().flatten());
        let text = |oid: Option<gix::ObjectId>| -> Result<String, String> {
            let Some(o) = oid else {
                return Ok(String::new());
            };
            Ok(crate::git::repo::blob_text(repo, o)?.unwrap_or_default())
        };
        let stat = classify_file(
            &text(base_oid)?,
            &text(Some(tip_oid))?,
            &text(head_oid)?,
            head_oid != base_oid,
        );
        if stat.stray > 0 {
            files.push((path.clone(), stat));
        }
    }
    files.sort_by(|a, b| b.1.stray.cmp(&a.1.stray).then_with(|| a.0.cmp(&b.0)));
    let stray: usize = files.iter().map(|(_, f)| f.stray).sum();
    let superseded: usize = files
        .iter()
        .filter(|(_, f)| f.superseded)
        .map(|(_, f)| f.stray)
        .sum();
    let verdict = branch_verdict(stray, superseded);
    let (tip, date) = crate::git::repo::commit_stamp(repo, branch)?.unwrap_or_default();
    let file_rows: Vec<Value> = files
        .iter()
        .map(|(path, f)| {
            json!({
                "p": path,
                "verdict": f.verdict(),
                "stray": f.stray,
                "authored": f.authored,
                "del": f.del,
                "redone": f.redone,
                "sim": f.sim,
                "head_touched": f.head_touched,
            })
        })
        .collect();
    Ok(Some(json!({
        "name": branch,
        "tip": tip,
        "date": date,
        "base": crate::git::repo::short_id(base),
        "verdict": verdict.as_str(),
        "stray": stray,
        "superseded": superseded,
        "files": file_rows,
    })))
}

/// Counts of branch verdicts across one `--stray-content` sweep.
#[derive(Default)]
struct StrayTally {
    refs: usize,
    unmerged: usize,
    superseded: usize,
    merged: usize,
    unknown: usize,
    branches: Vec<Value>,
}

impl StrayTally {
    fn add(&mut self, report: Option<Value>) {
        self.refs += 1;
        let Some(report) = report else {
            self.unknown += 1;
            return;
        };
        match report["verdict"].as_str() {
            Some("superseded") => self.superseded += 1,
            Some("merged") => self.merged += 1,
            _ => self.unmerged += 1,
        }
        if report["stray"].as_u64() != Some(0) {
            self.branches.push(report);
        }
    }
}

/// Whether `branch` is a divergent ref to scan: not HEAD, not the checked-out branch, and not
/// sitting at HEAD's commit (that ref is the live line itself).
#[must_use]
pub fn is_stray_candidate<T: PartialEq>(
    branch: &str,
    head_ref: &str,
    branch_tip: Option<T>,
    head_commit: Option<T>,
) -> bool {
    !(branch.is_empty() || branch == "HEAD" || branch == head_ref) && branch_tip != head_commit
}

/// `--stray-content` JSON from per-ref reports. `reports` holds one entry per scanned ref, `None`
/// for a ref that could not be analysed; `blobs` counts the distinct blobs read.
#[must_use]
pub fn stray_content_value(
    at: String,
    head_sha: String,
    head_ref: String,
    blobs: usize,
    reports: Vec<Option<Value>>,
) -> Value {
    let mut tally = StrayTally::default();
    for report in reports {
        tally.add(report);
    }
    json!({
        "head": head_sha,
        "head_ref": head_ref,
        "refs": tally.refs,
        "blobs": blobs,
        "unmerged": tally.unmerged,
        "superseded": tally.superseded,
        "merged": tally.merged,
        "unknown": tally.unknown,
        "at": at,
        "branches": tally.branches
    })
}

/// Per-ref stray reports and the distinct blob count, gathered from the repository.
fn gather_stray_reports(
    repo: &gix::Repository,
    head_ref: &str,
) -> Result<(Vec<Option<Value>>, usize), String> {
    let head_blobs = crate::git::repo::path_blobs(repo, "HEAD")?;
    let head_commit = crate::git::repo::rev_id(repo, "HEAD");
    let mut blobs = HashSet::new();
    let mut reports = Vec::new();
    for branch in crate::git::repo::local_branches(repo)? {
        let tip = crate::git::repo::rev_id(repo, &branch);
        if is_stray_candidate(&branch, head_ref, tip, head_commit) {
            reports.push(branch_stray_report(repo, &branch, &head_blobs, &mut blobs)?);
        }
    }
    Ok((reports, blobs.len()))
}

/// Execute `stray_content` tool.
pub fn call_stray_content(root: &str) -> Result<Value, String> {
    let at_stamp = crate::git::history::compute_git_at_stamp(root)?.unwrap_or_default();
    let repo = crate::git::repo::discover(root);
    let head_sha = repo
        .as_ref()
        .and_then(|r| r.head_id().ok())
        .map(crate::git::repo::short_id)
        .unwrap_or_default();
    let head_ref = repo
        .as_ref()
        .map_or_else(|| "HEAD".to_string(), crate::git::repo::head_ref);
    let gathered = match &repo {
        Some(repo) => gather_stray_reports(repo, &head_ref)?,
        None => (Vec::new(), 0),
    };
    let (reports, blobs) = gathered;
    Ok(stray_content_value(
        at_stamp, head_sha, head_ref, blobs, reports,
    ))
}
