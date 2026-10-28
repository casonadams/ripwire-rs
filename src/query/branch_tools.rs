use crate::domain::{IngestResult, Interner};
use crate::query::stray_lines::{branch_verdict, classify_file};
use crate::query::{FileSources, SourceReader};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Execute `owners` tool: git commit author ownership per file.
fn record_commit_authors(
    file_authors: &mut HashMap<String, HashMap<String, f64>>,
    files: &[String],
    author: &str,
    weight: f64,
    path_filter: Option<&str>,
    known_files: Option<&std::collections::HashSet<String>>,
) {
    for f in files {
        if let Some(known) = known_files
            && !known.contains(f)
        {
            continue;
        }
        if path_filter.is_none() || path_filter.is_some_and(|pf| f.contains(pf)) {
            let entry = file_authors.entry(f.clone()).or_default();
            *entry.entry(author.to_string()).or_insert(0.0) += weight;
        }
    }
}

/// Execute `owners` tool: git commit author ownership per file.
fn compute_file_author_weights(
    commits: &[crate::git::history::CommitInfo],
    path_filter: Option<&str>,
    known_files: Option<&std::collections::HashSet<String>>,
) -> HashMap<String, HashMap<String, f64>> {
    let mut file_authors: HashMap<String, HashMap<String, f64>> = HashMap::new();
    let head_epoch = commits.first().map_or(0, |c| c.timestamp);

    for c in commits {
        let weight = recency_weight(head_epoch, c.timestamp);
        let author = if c.author.is_empty() {
            "unknown"
        } else {
            &c.author
        };
        record_commit_authors(
            &mut file_authors,
            &c.changed_files,
            author,
            weight,
            path_filter,
            known_files,
        );
    }
    file_authors
}

/// Weight of a commit `timestamp` seconds old relative to `head_epoch`: halves every 180 days.
#[must_use]
pub fn recency_weight(head_epoch: i64, timestamp: i64) -> f64 {
    let age_days = ((head_epoch - timestamp).max(0) as f64) / 86400.0;
    0.5f64.powf(age_days / 180.0)
}

/// `bf=`: 1 when one author holds at least 80% of the file's recency-weighted commits, else 0.
#[must_use]
pub fn single_owner_flag(top_share: f64) -> usize {
    usize::from(top_share >= 0.80)
}

fn score_file_ownership(f: String, author_weights: HashMap<String, f64>) -> Value {
    let authors_count = author_weights.len();
    let total_weight: f64 = author_weights.values().sum();
    let (top_author, top_weight) = author_weights
        .into_iter()
        .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or_default();
    let share = if total_weight > 0.0 {
        top_weight / total_weight
    } else {
        1.0
    };
    let bf = single_owner_flag(share);

    json!({
        "file": f.clone(),
        "path": f,
        "authors": authors_count,
        "bf": bf,
        "top": top_author.clone(),
        "top_author": top_author,
        "share": (share * 100.0).round() / 100.0,
        "top_share": (share * 100.0).round() / 100.0,
    })
}

/// Execute `owners` tool: git commit author ownership per file.
pub fn call_owners(
    commits: Option<&[crate::git::history::CommitInfo]>,
    path_filter: Option<&str>,
    at_stamp: Option<&str>,
    known_files: Option<&std::collections::HashSet<String>>,
) -> Value {
    let Some(commits) = commits else {
        return json!({ "error": "git commit history unavailable" });
    };

    let file_authors = compute_file_author_weights(commits, path_filter, known_files);
    let mut file_results: Vec<Value> = file_authors
        .into_iter()
        .map(|(f, map)| score_file_ownership(f, map))
        .collect();

    file_results.sort_by(|a, b| {
        let path_a = a["path"].as_str().unwrap_or("");
        let path_b = b["path"].as_str().unwrap_or("");
        path_a.cmp(path_b)
    });

    json!({
        "files_analyzed": file_results.len(),
        "files": file_results.clone(),
        "ownership": file_results,
        "at": at_stamp
    })
}

fn branch_hit_json(
    branch: &str,
    stamp: &(String, String),
    symbol_name: &str,
    hit: &crate::git::repo::LineHit,
    kind: &str,
) -> Value {
    let code_text = hit.text.trim();
    json!({
        "ref": branch,
        "tip": stamp.0,
        "date": stamp.1,
        "symbol": symbol_name,
        "kind": kind,
        "file": hit.path,
        "path": hit.path,
        "line": hit.line,
        "text": code_text,
        "on_head": false
    })
}

struct WhereisScan {
    rows: Vec<Value>,
    on_head: Option<bool>,
    refs_scanned: usize,
    blobs: usize,
}

fn scan_repo_whereis(
    repo: &gix::Repository,
    root: &str,
    sources: &(impl SourceReader + ?Sized),
    symbol_name: &str,
    index_defs: &HashMap<&str, HashSet<u32>>,
) -> WhereisScan {
    let scope = crate::git::repo::workdir_prefix(repo, root).unwrap_or_default();
    let head = repo.head_id().ok().map(gix::Id::detach);
    let branches: Vec<String> = crate::git::repo::local_branches(repo)
        .into_iter()
        .filter(|b| repo.rev_parse_single(b.as_str()).ok().map(gix::Id::detach) != head)
        .collect();
    let revs: Vec<&str> = std::iter::once("HEAD")
        .chain(branches.iter().map(String::as_str))
        .collect();
    let grep = crate::git::repo::grep_word_revs(repo, &revs, &scope, symbol_name);
    let mut per_rev = grep.per_rev.into_iter();
    let head_hits = per_rev.next().unwrap_or_default();
    let changed: Vec<String> = crate::git::repo::dirty_paths(root)
        .into_iter()
        .filter_map(|p| p.strip_prefix(scope.as_str()).map(str::to_string))
        .collect();
    let mut rows: Vec<Value> = head_hits
        .iter()
        .filter(|h| changed.binary_search(&h.path).is_err())
        .map(|h| {
            let src = CheckoutLine {
                label: Some("HEAD"),
                path: &h.path,
                line: h.line,
                text: &h.text,
            };
            checkout_row(&src, symbol_name, index_defs.get(h.path.as_str()))
        })
        .collect();
    for path in &changed {
        let defs = index_defs.get(path.as_str());
        rows.extend(scan_file_whereis_hits(
            sources,
            path,
            symbol_name,
            defs,
            Some("worktree"),
        ));
    }
    sort_checkout_rows(&mut rows);
    let on_head = Some(!rows.is_empty());
    let per_branch: Vec<Vec<crate::git::repo::LineHit>> = per_rev.collect();
    rows.extend(branch_rows(repo, &branches, &per_branch, symbol_name));
    WhereisScan {
        rows,
        on_head,
        refs_scanned: branches.len(),
        blobs: grep.blobs,
    }
}

fn branch_rows(
    repo: &gix::Repository,
    branches: &[String],
    per_branch: &[Vec<crate::git::repo::LineHit>],
    symbol_name: &str,
) -> Vec<Value> {
    let def_lines = blob_def_lines(repo, per_branch, symbol_name);
    let mut rows = Vec::new();
    for (branch, hits) in branches.iter().zip(per_branch) {
        let stamp = crate::git::repo::commit_stamp(repo, branch).unwrap_or_default();
        rows.extend(hits.iter().map(|hit| {
            let kind = if def_lines[&hit.blob].contains(&(hit.line as u32)) {
                "def"
            } else {
                "ref"
            };
            branch_hit_json(branch, &stamp, symbol_name, hit, kind)
        }));
    }
    rows
}

fn blob_def_lines(
    repo: &gix::Repository,
    per_branch: &[Vec<crate::git::repo::LineHit>],
    symbol_name: &str,
) -> HashMap<gix::ObjectId, Vec<u32>> {
    use rayon::prelude::*;
    let mut first_path: HashMap<gix::ObjectId, &str> = HashMap::new();
    for hit in per_branch.iter().flatten() {
        first_path.entry(hit.blob).or_insert(&hit.path);
    }
    let sync_repo = repo.clone().into_sync();
    first_path
        .into_par_iter()
        .map_init(
            || sync_repo.to_thread_local(),
            |r, (oid, path)| {
                let defs = r.find_blob(oid).map_or_else(
                    |_| Vec::new(),
                    |b| crate::ingest::definition_lines(path, &b.data, symbol_name),
                );
                (oid, defs)
            },
        )
        .collect()
}

pub(super) fn probe_name_history(root: &str, sym_name: &str, head_sha: &str) -> Option<Value> {
    let repo = crate::git::repo::discover(root)?;
    if let Some(commit) = crate::git::repo::last_count_change(&repo, sym_name) {
        return Some(json!({
            "probed": 1,
            "head": head_sha,
            "fate": {
                "v": "removed",
                "commit": commit.short_id,
                "date": commit.date,
                "subject": commit.subject
            }
        }));
    }
    Some(json!({
        "probed": 1,
        "head": head_sha,
        "fate": {
            "v": "never",
            "note": "no commit reachable from HEAD ever removed a line carrying this name \u{2014} so for a name HEAD does not have, this repo never had it"
        }
    }))
}

struct CheckoutLine<'a> {
    label: Option<&'a str>,
    path: &'a str,
    line: usize,
    text: &'a str,
}

fn checkout_row(src: &CheckoutLine<'_>, symbol_name: &str, defs: Option<&HashSet<u32>>) -> Value {
    let line = src.line as u32;
    let is_def = defs.is_some_and(|s| s.contains(&line));
    let code_text: String = src.text.trim().chars().take(200).collect();
    let mut row = json!({
        "symbol": symbol_name,
        "kind": if is_def { "def" } else { "ref" },
        "file": src.path,
        "path": src.path,
        "line": line,
        "text": code_text,
        "is_def": is_def,
    });
    if let Some(label) = src.label {
        row["ref"] = json!(label);
        row["on_head"] = json!(true);
    }
    row
}

fn scan_file_whereis_hits(
    sources: &(impl SourceReader + ?Sized),
    rel_path: &str,
    symbol_name: &str,
    defs: Option<&HashSet<u32>>,
    label: Option<&str>,
) -> Vec<Value> {
    let Ok(bytes) = sources.read(rel_path) else {
        return Vec::new();
    };
    let Ok(content) = std::str::from_utf8(&bytes) else {
        return Vec::new();
    };
    if !content.contains(symbol_name) {
        return Vec::new();
    }
    content
        .lines()
        .enumerate()
        .filter(|(_, text)| crate::git::repo::has_word(text, symbol_name))
        .map(|(idx, text)| {
            let src = CheckoutLine {
                label,
                path: rel_path,
                line: idx + 1,
                text,
            };
            checkout_row(&src, symbol_name, defs)
        })
        .collect()
}

fn scan_files_whereis(
    ingest: &IngestResult,
    interner: &Interner,
    sources: &(impl SourceReader + ?Sized),
    symbol_name: &str,
    index_defs: &HashMap<&str, HashSet<u32>>,
    label: Option<&str>,
) -> WhereisScan {
    let mut rows: Vec<Value> = ingest
        .files
        .iter()
        .flat_map(|fi| {
            let path = interner.resolve(&fi.path);
            scan_file_whereis_hits(sources, path, symbol_name, index_defs.get(path), label)
        })
        .collect();
    sort_checkout_rows(&mut rows);
    WhereisScan {
        on_head: None,
        rows,
        refs_scanned: 0,
        blobs: ingest.files.len(),
    }
}

fn sort_checkout_rows(rows: &mut [Value]) {
    rows.sort_by(|a, b| {
        let a_def = a["is_def"].as_bool().unwrap_or(false);
        let b_def = b["is_def"].as_bool().unwrap_or(false);
        b_def
            .cmp(&a_def)
            .then_with(|| a["path"].as_str().cmp(&b["path"].as_str()))
            .then_with(|| a["line"].as_u64().cmp(&b["line"].as_u64()))
    });
}

#[must_use]
pub fn index_def_lines<'a>(
    ingest: &IngestResult,
    interner: &'a Interner,
    symbol_name: &str,
) -> HashMap<&'a str, HashSet<u32>> {
    let mut defs: HashMap<&str, HashSet<u32>> = HashMap::new();
    for sym in &ingest.symbols {
        if interner.resolve(&sym.name) == symbol_name
            && let Some(file) = ingest.files.get(sym.file_id as usize)
        {
            defs.entry(interner.resolve(&file.path))
                .or_default()
                .insert(sym.line);
        }
    }
    defs
}

pub fn validate_whereis_listing(listing: Option<&str>) -> Result<(), String> {
    if let Some(listing) = listing
        && !matches!(listing, "defs" | "refs" | "all")
    {
        return Err(format!(
            "invalid whereis listing '{listing}': expected defs, refs, or all"
        ));
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
pub struct WhereisRequest<'a> {
    pub root: &'a str,
    pub symbol_name: &'a str,
    pub with_history: bool,
}

#[derive(Debug, Clone, Default)]
pub struct WhereisFacts {
    pub history: Option<Value>,
    pub head_date: Option<String>,
    pub at_stamp: Option<String>,
    pub branch_rows: Vec<Value>,
    pub refs_scanned: usize,
}

pub fn whereis_from_sources_with_facts(
    sources: &(impl SourceReader + ?Sized),
    ingest: &IngestResult,
    interner: &Interner,
    symbol_name: &str,
    facts: &WhereisFacts,
) -> Value {
    let defs = index_def_lines(ingest, interner, symbol_name);
    let git_backed = facts.at_stamp.is_some();
    let mut scan = scan_files_whereis(
        ingest,
        interner,
        sources,
        symbol_name,
        &defs,
        git_backed.then_some("HEAD"),
    );
    if git_backed {
        scan.on_head = Some(!scan.rows.is_empty());
    }
    scan.rows.extend(facts.branch_rows.clone());
    scan.refs_scanned += facts.refs_scanned;
    whereis_report(
        symbol_name,
        scan,
        facts.history.clone(),
        facts.head_date.as_deref().unwrap_or(""),
        facts.at_stamp.as_deref().unwrap_or(""),
    )
}

pub fn whereis_from_sources(
    sources: &(impl SourceReader + ?Sized),
    ingest: &IngestResult,
    interner: &Interner,
    symbol_name: &str,
) -> Value {
    whereis_from_sources_with_facts(
        sources,
        ingest,
        interner,
        symbol_name,
        &WhereisFacts::default(),
    )
}

pub fn whereis_with_sources_and_history(
    req: WhereisRequest<'_>,
    sources: &(impl SourceReader + ?Sized),
    ingest: &IngestResult,
    interner: &Interner,
) -> Value {
    let at_stamp = crate::git::history::compute_git_at_stamp(req.root).unwrap_or_default();
    let repo = crate::git::repo::discover(req.root);
    let head_date = repo
        .as_ref()
        .and_then(|repo| crate::git::repo::commit_stamp(repo, "HEAD"))
        .map(|(_, date)| date)
        .unwrap_or_default();
    let index_defs = index_def_lines(ingest, interner, req.symbol_name);
    let scan = match &repo {
        Some(repo) => scan_repo_whereis(repo, req.root, sources, req.symbol_name, &index_defs),
        None => scan_files_whereis(
            ingest,
            interner,
            sources,
            req.symbol_name,
            &index_defs,
            None,
        ),
    };
    let history_info = if req.with_history && scan.on_head == Some(false) {
        probe_name_history(req.root, req.symbol_name, &at_stamp)
    } else {
        None
    };
    whereis_report(req.symbol_name, scan, history_info, &head_date, &at_stamp)
}

/// Execute `whereis` tool: identify where a symbol lives (definitions and references).
pub fn call_whereis(
    ingest: &IngestResult,
    interner: &Interner,
    root: &str,
    symbol_name: &str,
    with_history: bool,
) -> Value {
    whereis_with_sources_and_history(
        WhereisRequest {
            root,
            symbol_name,
            with_history,
        },
        &FileSources {
            root: std::path::Path::new(root),
        },
        ingest,
        interner,
    )
}

fn whereis_report(
    symbol_name: &str,
    scan: WhereisScan,
    history_info: Option<Value>,
    head_date: &str,
    at_stamp: &str,
) -> Value {
    let hits_count = scan.rows.len();
    json!({
        "symbol": symbol_name,
        "on_head": scan.on_head,
        "git": scan.on_head.is_some(),
        "refs_scanned": scan.refs_scanned,
        "blobs": scan.blobs,
        "hits_count": hits_count,
        "count": hits_count,
        "hits": scan.rows.clone(),
        "locations": scan.rows,
        "history": history_info,
        "head_date": head_date,
        "at": at_stamp
    })
}

/// Per-branch stray report: merge-base relation, verdict, and per-file evidence sorted by stray.
fn branch_stray_report(
    repo: &gix::Repository,
    branch: &str,
    head_blobs: &BTreeMap<String, gix::ObjectId>,
    blobs: &mut HashSet<gix::ObjectId>,
) -> Option<Value> {
    let base = crate::git::repo::merge_base_id(repo, branch)?;
    let base_blobs = crate::git::repo::path_blobs(repo, &base.to_hex().to_string());
    let tip_blobs = crate::git::repo::path_blobs(repo, branch);
    let mut files = Vec::new();
    for (path, &tip_oid) in &tip_blobs {
        let base_oid = base_blobs.get(path).copied();
        if base_oid == Some(tip_oid) {
            continue;
        }
        let head_oid = head_blobs.get(path).copied();
        blobs.extend([Some(tip_oid), base_oid, head_oid].into_iter().flatten());
        let text = |oid: Option<gix::ObjectId>| {
            oid.map_or_else(String::new, |o| crate::git::repo::blob_text(repo, o))
        };
        let stat = classify_file(
            &text(base_oid),
            &text(Some(tip_oid)),
            &text(head_oid),
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
    let (tip, date) = crate::git::repo::commit_stamp(repo, branch).unwrap_or_default();
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
    Some(json!({
        "name": branch,
        "tip": tip,
        "date": date,
        "base": crate::git::repo::short_id(base),
        "verdict": verdict.as_str(),
        "stray": stray,
        "superseded": superseded,
        "files": file_rows,
    }))
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
fn gather_stray_reports(repo: &gix::Repository, head_ref: &str) -> (Vec<Option<Value>>, usize) {
    let head_blobs = crate::git::repo::path_blobs(repo, "HEAD");
    let head_commit = crate::git::repo::rev_id(repo, "HEAD");
    let mut blobs = HashSet::new();
    let reports = crate::git::repo::local_branches(repo)
        .into_iter()
        .filter(|branch| {
            let tip = crate::git::repo::rev_id(repo, branch);
            is_stray_candidate(branch, head_ref, tip, head_commit)
        })
        .map(|branch| branch_stray_report(repo, &branch, &head_blobs, &mut blobs))
        .collect();
    (reports, blobs.len())
}

/// Execute `stray_content` tool.
pub fn call_stray_content(root: &str) -> Value {
    let at_stamp = crate::git::history::compute_git_at_stamp(root).unwrap_or_default();
    let repo = crate::git::repo::discover(root);
    let head_sha = repo
        .as_ref()
        .and_then(|r| r.head_id().ok())
        .map(crate::git::repo::short_id)
        .unwrap_or_default();
    let head_ref = repo
        .as_ref()
        .map_or_else(|| "HEAD".to_string(), crate::git::repo::head_ref);
    let (reports, blobs) = repo
        .as_ref()
        .map_or_else(|| (Vec::new(), 0), |r| gather_stray_reports(r, &head_ref));
    stray_content_value(at_stamp, head_sha, head_ref, blobs, reports)
}
