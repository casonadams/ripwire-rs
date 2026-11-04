mod scan;

use self::scan::{WhereisScan, scan_files_whereis, scan_repo_whereis};
use super::history::probe_name_history;
use crate::domain::{IngestResult, Interner};
use crate::query::{FileSources, SourceReader};
use serde_json::{Value, json};
use std::collections::{HashMap, HashSet};

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

pub fn whereis_with_sources_and_history(
    req: WhereisRequest<'_>,
    sources: &(impl SourceReader + ?Sized),
    ingest: &IngestResult,
    interner: &Interner,
) -> Result<Value, String> {
    let at_stamp = crate::git::history::compute_git_at_stamp(req.root)?.unwrap_or_default();
    let repo = crate::git::repo::discover(req.root);
    let head_date = match &repo {
        Some(repo) => crate::git::repo::commit_stamp(repo, "HEAD")?
            .map(|(_, date)| date)
            .unwrap_or_default(),
        None => String::new(),
    };
    let index_defs = index_def_lines(ingest, interner, req.symbol_name);
    let scan = match &repo {
        Some(repo) => scan_repo_whereis(repo, req.root, sources, req.symbol_name, &index_defs)?,
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
    Ok(whereis_report(
        req.symbol_name,
        scan,
        history_info,
        &head_date,
        &at_stamp,
    ))
}

/// Execute `whereis` tool: identify where a symbol lives (definitions and references).
pub fn call_whereis(
    ingest: &IngestResult,
    interner: &Interner,
    root: &str,
    symbol_name: &str,
    with_history: bool,
) -> Result<Value, String> {
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
