//! `--handoff`: continuation packet for interrupted agent sessions.
use super::{Plugin, Run};
use crate::commands::context::ExecutionContext;
use crate::domain::{IngestResult, Interner, NodeId};
use crate::git::cochange::{CoPartner, CochangeOptions, StaticCouplingMap, mine_cochange_for_file};
use crate::git::history::{
    DEFAULT_COCHANGE_MONTHS, compute_git_at_stamp, resolve_cutoff_timestamp,
    walk_repository_history_scoped,
};
use crate::graph::Graph;
use crate::serialize::{PageRequest, Paged, XmlEscapeExt, text_bytes};
use clap::Args;
use std::collections::BTreeSet;
use std::io::{self, Write};
use std::path::Path;

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct HandoffArgs {
    /// Continuation packet for interrupted agent sessions
    #[arg(long)]
    pub handoff: bool,
}

pub static HANDOFF: Plugin<HandoffArgs> = Plugin {
    name: "handoff",
    options: &[],
    selected: |args| args.handoff,
    run,
    check: None,
    tools: &[],
};

fn run(run: &Run<'_>, _: &HandoffArgs) -> io::Result<()> {
    run.report(|ctx, out| writeln!(out, "{}", build_handoff_xml(ctx, run.page())?))
}

/// Branch, head subject, and detached state of the repository enclosing `root`; `None` outside git.
struct GitHead {
    branch: String,
    subject: Option<String>,
    detached: bool,
}

fn resolve_git_head(root: &str) -> io::Result<Option<GitHead>> {
    let Some(repo) = crate::git::repo::enclosing_repo(root).map_err(io::Error::other)? else {
        return Ok(None);
    };
    let branch_opt = repo
        .head_name()
        .map_err(|e| io::Error::other(e.to_string()))?
        .map(|n| {
            let s = n.as_bstr().to_string();
            s.strip_prefix("refs/heads/").unwrap_or(&s).to_string()
        });
    let detached = branch_opt.is_none();
    let branch = branch_opt.unwrap_or_else(|| "HEAD".to_string());
    let subject = repo
        .head_commit()
        .ok()
        .and_then(|c| {
            c.message_raw()
                .ok()
                .and_then(|m| std::str::from_utf8(m.as_ref()).ok().map(str::to_string))
        })
        .and_then(|m| m.lines().next().map(|l| l.trim().to_string()))
        .filter(|s| !s.is_empty());
    Ok(Some(GitHead {
        branch,
        subject,
        detached,
    }))
}

fn changed_file_matches(changed: &str, path: &str) -> bool {
    path == changed || changed.ends_with(path)
}

fn changed_symbol_ids(
    changed: &[String],
    ingest: &IngestResult,
    interner: &Interner,
) -> Vec<NodeId> {
    ingest
        .symbols
        .iter()
        .filter(|s| {
            let p = ingest.sym_file_path(s, interner);
            changed.iter().any(|cf| changed_file_matches(cf, p))
        })
        .map(|s| s.id)
        .collect()
}

pub fn build_handoff_verified_rows(
    changed: &[String],
    ingest: &IngestResult,
    interner: &Interner,
    page: PageRequest,
) -> String {
    let shown = Paged::new(changed, page, |cf| text_bytes(&[cf.as_str()]));
    let mut rows = String::new();
    for cf in shown.rows() {
        let sym = ingest
            .symbols
            .iter()
            .find(|s| changed_file_matches(cf, ingest.sym_file_path(s, interner)));
        match sym {
            Some(s) => rows.push_str(&format!(
                "<f p=\"{}\"><s n=\"{}\"/></f>",
                cf.xml_escaped(),
                interner[s.name].xml_escaped()
            )),
            None => rows.push_str(&format!("<f p=\"{}\"/>", cf.xml_escaped())),
        }
    }
    rows
}

/// Files outside the symbols of `changed` that transitively call them: the change's blast radius in files.
pub fn handoff_blast_files(
    changed: &[String],
    ingest: &IngestResult,
    graph: &Graph,
    ranks: &[f64],
    interner: &Interner,
) -> usize {
    let seeds = changed_symbol_ids(changed, ingest, interner);
    crate::navigate::transitive_impact_multi(&seeds, ingest, graph, ranks, interner, None)
        .iter()
        .map(|n| n.path.as_str())
        .collect::<BTreeSet<_>>()
        .len()
}

/// Test files the changed files reach, by the same walk as `--affected`.
pub fn handoff_affected_tests(
    changed: &[String],
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
) -> usize {
    let refs: Vec<&str> = changed.iter().map(String::as_str).collect();
    crate::gate::affected::find_affected_tests(&refs, ingest, graph, interner)
        .tests
        .len()
}

fn build_handoff_doc_rows(root: &str, query: &str) -> (String, usize) {
    let inputs = crate::query::retrieval::collect_recall_inputs(Path::new(root));
    let recall = crate::query::retrieval::call_memory_recall(&inputs, query, Some(4), None);
    let mut rows = String::new();
    let mut cnt = 0usize;
    if let Some(docs) = recall.get("docs").and_then(|v| v.as_array()) {
        for d in docs {
            let p = d.get("path").and_then(|v| v.as_str()).unwrap_or("");
            let sc = d
                .get("score")
                .and_then(serde_json::Value::as_f64)
                .unwrap_or(1.0);
            if !p.is_empty() {
                rows.push_str(&format!("<doc p=\"{}\" s=\"{sc:.3}\"/>", p.xml_escaped()));
                cnt += 1;
            }
        }
    }
    (rows, cnt)
}

/// Co-change partners of `probe` mined from the git window, with the probed commit count they are scored against.
fn mine_cochange(
    root: &str,
    probe: &str,
    ingest: &IngestResult,
    graph: &Graph,
    interner: &Interner,
) -> io::Result<(Vec<CoPartner>, usize)> {
    let cutoff = resolve_cutoff_timestamp(None, DEFAULT_COCHANGE_MONTHS);
    let commits = walk_repository_history_scoped(root, cutoff).map_err(io::Error::other)?;
    let opts = CochangeOptions::default();
    let probed = commits
        .iter()
        .filter(|c| {
            c.changed_files.len() <= opts.max_commit_files
                && c.changed_files.iter().any(|f| f == probe)
        })
        .count();
    let coupling = StaticCouplingMap::build(ingest, interner, Some(graph));
    let partners = mine_cochange_for_file(
        probe,
        &commits,
        Some(&coupling),
        Some(ingest),
        Some(interner),
        &opts,
    );
    Ok((partners, probed))
}

/// Co-change partners of the first changed file, and the window attributes that describe their mining.
/// Inside a repository the window is always mined; with no changed file the probed count is zero.
fn handoff_cochange(
    root: &str,
    changed: &[String],
    head: Option<&GitHead>,
    ctx: &ExecutionContext<'_>,
) -> io::Result<(Vec<CoPartner>, String)> {
    if head.is_none() {
        return Ok((Vec::new(), String::new()));
    }
    let (partners, probed) = match changed.first() {
        Some(probe) => mine_cochange(root, probe, ctx.ingest, ctx.graph, ctx.interner)?,
        None => (Vec::new(), 0),
    };
    Ok((
        partners,
        format!(" cochange_window=\"{DEFAULT_COCHANGE_MONTHS}mo@HEAD\" cochange_commits=\"{probed}\""),
    ))
}

struct HeuristicRows {
    rows: String,
    shown: usize,
    candidates: usize,
    capped: bool,
}

fn handoff_heuristic_rows(
    root: &str,
    partners: &[CoPartner],
    head: Option<&GitHead>,
    page: PageRequest,
) -> HeuristicRows {
    let partners_shown = Paged::new(partners, page, |p| text_bytes(&[p.file_path.as_str()]));
    let mut rows = String::new();
    let mut shown = 0usize;
    for p in partners_shown.rows() {
        rows.push_str(&format!(
            "<f p=\"{}\" score=\"{:.2}\" why=\"cochange\"/>",
            p.file_path.xml_escaped(),
            p.deg
        ));
        shown += 1;
    }
    let (doc_rows, doc_cnt) = match head {
        Some(h) => build_handoff_doc_rows(
            root,
            &format!("{} {}", h.branch, h.subject.as_deref().unwrap_or("")),
        ),
        None => (String::new(), 0),
    };
    rows.push_str(&doc_rows);
    HeuristicRows {
        rows,
        shown: shown + doc_cnt,
        candidates: partners.len() + doc_cnt,
        capped: partners_shown.window.capped,
    }
}

/// The `at`, `root`, and git attributes of the handoff element; git attributes appear only inside a repository.
fn handoff_header_attrs(root: &str, head: Option<&GitHead>) -> String {
    let at = crate::serialize::git_at_attr(compute_git_at_stamp(root));
    let git = head.map_or(String::new(), |h| {
        let detached = if h.detached { " detached=\"1\"" } else { "" };
        let subject = h
            .subject
            .as_ref()
            .map_or(String::new(), |s| format!(" subject=\"{}\"", s.xml_escaped()));
        format!(
            " branch=\"{}\"{detached}{subject} gitok=\"1\"",
            h.branch.xml_escaped()
        )
    });
    format!("{at} root=\"{root}\"{git}")
}

pub fn build_handoff_xml(ctx: &ExecutionContext<'_>, page: PageRequest) -> io::Result<String> {
    let root = ctx.root;
    let head = resolve_git_head(root)?;
    let changed = if head.is_some() {
        crate::commands::common::changes::get_working_tree_changed_files(root)?
    } else {
        Vec::new()
    };
    let verified_rows = build_handoff_verified_rows(&changed, ctx.ingest, ctx.interner, page);
    let blast_files = handoff_blast_files(
        &changed,
        ctx.ingest,
        ctx.graph,
        &ctx.pr.ranks,
        ctx.interner,
    );
    let tests = handoff_affected_tests(&changed, ctx.ingest, ctx.graph, ctx.interner);
    let (partners, cochange_attr) = handoff_cochange(root, &changed, head.as_ref(), ctx)?;
    let heur = handoff_heuristic_rows(root, &partners, head.as_ref(), page);
    let est_tokens = (verified_rows.len() + heur.rows.len() + 240) / 4;
    let header = handoff_header_attrs(root, head.as_ref());
    Ok(format!(
        "<!-- ripwire-rs handoff schema=ripwire-rs.handoff/v1: session handoff bundle. -->\n<handoff schema=\"ripwire-rs.handoff/v1\"{header} est_tokens=\"{est_tokens}\"><verified changed=\"{}\" blast_files=\"{blast_files}\">{verified_rows}<tests n=\"{tests}\"/></verified><heuristic n=\"{}\" candidates=\"{}\" capped=\"{}\"{cochange_attr}>{}</heuristic></handoff>",
        changed.len(),
        heur.shown,
        heur.candidates,
        usize::from(heur.capped),
        heur.rows,
    ))
}
