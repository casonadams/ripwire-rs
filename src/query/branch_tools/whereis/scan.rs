use crate::domain::{IngestResult, Interner};
use crate::query::SourceReader;
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

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

pub(super) struct WhereisScan {
    pub(super) rows: Vec<Value>,
    pub(super) on_head: Option<bool>,
    pub(super) refs_scanned: usize,
    pub(super) blobs: usize,
}

pub(super) fn scan_repo_whereis(
    repo: &gix::Repository,
    root: &str,
    sources: &(impl SourceReader + ?Sized),
    symbol_name: &str,
    index_defs: &HashMap<&str, HashSet<u32>>,
) -> Result<WhereisScan, String> {
    let scope = crate::git::repo::workdir_prefix(repo, root)?;
    let head = repo.head_id().ok().map(gix::Id::detach);
    let branches: Vec<String> = crate::git::repo::local_branches(repo)?
        .into_iter()
        .filter(|b| repo.rev_parse_single(b.as_str()).ok().map(gix::Id::detach) != head)
        .collect();
    let revs: Vec<&str> = std::iter::once("HEAD")
        .chain(branches.iter().map(String::as_str))
        .collect();
    let grep = crate::git::repo::grep_word_revs(repo, &revs, &scope, symbol_name)?;
    let mut per_rev = grep.per_rev.into_iter();
    let head_hits = per_rev.next().unwrap_or_default();
    let changed: Vec<String> = crate::git::repo::dirty_paths(root)?
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
    rows.extend(branch_rows(repo, &branches, &per_branch, symbol_name)?);
    Ok(WhereisScan {
        rows,
        on_head,
        refs_scanned: branches.len(),
        blobs: grep.blobs,
    })
}

fn branch_rows(
    repo: &gix::Repository,
    branches: &[String],
    per_branch: &[Vec<crate::git::repo::LineHit>],
    symbol_name: &str,
) -> Result<Vec<Value>, String> {
    let def_lines = blob_def_lines(repo, per_branch, symbol_name);
    let mut rows = Vec::new();
    for (branch, hits) in branches.iter().zip(per_branch) {
        let stamp = crate::git::repo::commit_stamp(repo, branch)?.unwrap_or_default();
        rows.extend(hits.iter().map(|hit| {
            let kind = if def_lines[&hit.blob].contains(&(hit.line as u32)) {
                "def"
            } else {
                "ref"
            };
            branch_hit_json(branch, &stamp, symbol_name, hit, kind)
        }));
    }
    Ok(rows)
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

pub(super) fn scan_files_whereis(
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
