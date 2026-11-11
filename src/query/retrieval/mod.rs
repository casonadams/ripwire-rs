pub use super::grep::*;
pub use super::search::*;

use crate::domain::{IngestResult, Interner, Symbol};
use crate::graph::Graph;
use crate::query::lexical::Lexical;
use serde_json::{Value, json};

mod collect;
mod named_file;
mod ranking;
mod rows;
mod scoring;
mod terms;

use collect::{
    collect_for_callers, collect_for_compose, collect_for_doc_mentions, collect_for_hops,
    collect_for_lego, collect_for_tree_rows,
};
pub use ranking::compute_adaptive_cut;
use ranking::{
    compute_for_route_metadata, diversify_top_candidates, is_exact_name_query,
    promote_exact_anchors, term_coverage_percent,
};
use rows::{RowsRequest, body_disclosure, build_symbol_rows, symbol_row_bytes};
use scoring::{ScoringInputs, score_candidates};
use terms::is_for_stopword;

/// Share of a `--for` token budget the ranked rows may take. The one-hop, doc and tree sections are budgeted
/// from the rows' budget (a quarter each), so rows alone at the full budget make an answer of about twice it.
const FOR_ROWS_SHARE_PERCENT: usize = 50;

fn for_rows_budget_bytes(tokens: usize) -> usize {
    crate::query::map::token_budget_bytes(tokens) * FOR_ROWS_SHARE_PERCENT / 100
}

#[derive(Default, Clone, Copy)]
pub struct ForTaskOptions<'a> {
    pub root: &'a str,
    pub at: Option<&'a str>,
    pub sources: Option<&'a dyn crate::query::SourceReader>,
    pub in_scope: Option<&'a str>,
    pub sections: Option<&'a str>,
    pub signatures_only: bool,
    pub auto_bodies: bool,
    pub graph: Option<&'a Graph>,
    pub budget_tokens: Option<usize>,
    /// Window over the ranked symbols: `--limit` rows from `--offset`, shaped by the budget when unset.
    pub limit: Option<usize>,
    pub offset: Option<usize>,
    pub detail: Option<usize>,
    pub adaptive: bool,
}

/// Parse `--sections=`: a closed, comma-separated, order-insensitive set of `lego` and `compose`.
pub fn parse_for_sections(raw: &str) -> Result<Vec<&'static str>, String> {
    let mut out: Vec<&'static str> = Vec::new();
    for part in raw.split(',').map(str::trim) {
        let name = match part {
            "lego" => "lego",
            "compose" => "compose",
            "" => return Err(
                "ripwire-rs: --sections= names an empty section; use lego, compose, or lego,compose"
                    .to_string(),
            ),
            other => {
                return Err(format!(
                    "ripwire-rs: --sections={raw}: unknown section `{other}`; use lego, compose, or lego,compose"
                ));
            }
        };
        if out.contains(&name) {
            return Err(format!(
                "ripwire-rs: --sections={raw}: `{name}` named twice"
            ));
        }
        out.push(name);
    }
    Ok(out)
}

fn query_terms(task: &str) -> Vec<&str> {
    let all_terms: Vec<&str> = task
        .split_whitespace()
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric()))
        .filter(|w| w.len() >= 2)
        .collect();
    let filtered_terms: Vec<&str> = all_terms
        .iter()
        .copied()
        .filter(|w| !is_for_stopword(w))
        .collect();
    if filtered_terms.is_empty() {
        all_terms
    } else {
        filtered_terms
    }
}

fn rank_candidates<'a>(
    ingest: &'a IngestResult,
    interner: &'a Interner,
    ranks: &[f64],
    raw_terms: &[&str],
    lexical: Option<&Lexical>,
    in_scope: Option<&str>,
) -> Vec<(f64, &'a Symbol)> {
    let inputs = ScoringInputs {
        ranks,
        lexical,
        raw_terms,
    };
    let mut scored = score_candidates(ingest, interner, &inputs, in_scope);
    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    diversify_top_candidates(&mut scored, raw_terms, ingest, interner);
    if let [term] = raw_terms {
        promote_exact_anchors(&mut scored, term, interner);
    }
    scored
}

struct WindowSpan {
    begin: usize,
    end: usize,
    adaptive: Option<(usize, usize, f64)>,
}

fn select_window(
    scored: &[(f64, &Symbol)],
    ingest: &IngestResult,
    interner: &Interner,
    opts: ForTaskOptions<'_>,
    budget: Option<usize>,
) -> WindowSpan {
    let window = crate::serialize::PageRequest {
        limit: opts.limit,
        offset: opts.offset,
        budget,
    }
    .window_fit(scored.len(), |i| {
        symbol_row_bytes(scored[i].1, ingest, interner)
    });
    let mut span = WindowSpan {
        begin: window.begin,
        end: window.end,
        adaptive: None,
    };
    if opts.adaptive && window.shown > 5 {
        let scores: Vec<f64> = scored[span.begin..span.end].iter().map(|s| s.0).collect();
        let (kept, _cliff, gap) = compute_adaptive_cut(&scores, 5, scores.len());
        span.adaptive = Some((kept, scored.len(), gap));
        span.end = span.begin + kept;
    }
    span
}

fn window_json(span: &WindowSpan, total: usize, opts: ForTaskOptions<'_>) -> Value {
    json!({
        "begin": span.begin,
        "end": span.end,
        "total": total,
        "limit": opts.limit.unwrap_or(0),
        "offset": opts.offset.unwrap_or(0),
    })
}

struct SideLists {
    lego: Vec<Value>,
    compose: Vec<Value>,
    hops: Vec<Value>,
    callers: Option<Value>,
    tree: Vec<Value>,
    docs: Vec<Value>,
}

struct SideRequest<'a> {
    ingest: &'a IngestResult,
    interner: &'a Interner,
    opts: ForTaskOptions<'a>,
    ranks: &'a [f64],
    raw_terms: &'a [&'a str],
    exact_term: Option<&'a str>,
    scored: &'a [(f64, &'a Symbol)],
    span: &'a WindowSpan,
    budget: crate::serialize::PageRequest,
}

fn side_lists(req: &SideRequest<'_>) -> SideLists {
    let (ingest, interner) = (req.ingest, req.interner);
    let window_rows = &req.scored[req.span.begin..req.span.end];
    SideLists {
        lego: collect_for_lego(ingest, interner, window_rows),
        compose: collect_for_compose(ingest, interner, window_rows),
        hops: collect_for_hops(ingest, interner, req.opts.graph, window_rows, req.budget),
        callers: req.exact_term.and_then(|term| {
            collect_for_callers(
                ingest,
                interner,
                req.opts.graph,
                window_rows,
                term,
                req.ranks,
            )
        }),
        tree: collect_for_tree_rows(ingest, interner, req.scored, req.span.begin, req.span.end),
        docs: collect_for_doc_mentions(ingest, interner, req.raw_terms, req.budget),
    }
}

pub fn call_for_task_with_options(
    ingest: &IngestResult,
    interner: &Interner,
    ranks: &[f64],
    task: &str,
    opts: ForTaskOptions<'_>,
) -> Value {
    let sections = match opts.sections.map(parse_for_sections).transpose() {
        Ok(sections) => sections,
        Err(error) => return json!({ "error": error }),
    };
    let raw_terms = query_terms(task);
    let named = named_file::named_file(ingest, interner, ranks, task);
    let conceptual = named.is_some() || !is_exact_name_query(ingest, interner, &raw_terms);
    let lexical = (conceptual && named.is_none())
        .then(|| Lexical::build(ingest, interner, opts.sources, &raw_terms));
    let scored = match &named {
        Some(file) => file.scored.clone(),
        None => rank_candidates(
            ingest,
            interner,
            ranks,
            &raw_terms,
            lexical.as_ref(),
            opts.in_scope,
        ),
    };
    let budget_page = crate::serialize::PageRequest {
        limit: None,
        offset: None,
        budget: opts.budget_tokens.map(for_rows_budget_bytes),
    };
    let span = select_window(&scored, ingest, interner, opts, budget_page.budget);
    let exact_term = (raw_terms.len() == 1 && !conceptual).then(|| raw_terms[0]);
    let rows_request = RowsRequest {
        ingest,
        interner,
        opts,
        exact_term,
        budget: budget_page.budget,
    };
    let (symbols_json, wanted) =
        build_symbol_rows(&rows_request, span.begin, &scored[span.begin..span.end]);
    let served = symbols_json
        .iter()
        .filter(|s| s["body"].is_string())
        .count();
    let (bundle, reason, follow) = body_disclosure(opts, conceptual, &symbols_json, wanted, served);
    let (route, confidence, margin_pct, coverage) = match &named {
        Some(file) => (format!("file({})", file.path), "high", 100, 100),
        None => {
            let (route, confidence, margin_pct) =
                compute_for_route_metadata(task, &raw_terms, &scored, interner);
            let coverage = term_coverage_percent(&raw_terms, &scored, lexical.as_ref(), interner);
            (route, confidence, margin_pct, coverage)
        }
    };
    let side = side_lists(&SideRequest {
        ingest,
        interner,
        opts,
        ranks,
        raw_terms: &raw_terms,
        exact_term,
        scored: &scored,
        span: &span,
        budget: budget_page,
    });
    json!({
        "task": task,
        "root": opts.root,
        "at": opts.at,
        "route": route,
        "confidence": confidence,
        "margin_pct": margin_pct,
        "coverage": coverage,
        "symbols": symbols_json,
        "count": symbols_json.len(),
        "window": window_json(&span, scored.len(), opts),
        "detail": wanted,
        "bundle": bundle,
        "reason": reason,
        "follow": follow,
        "sections": sections,
        "lego": side.lego,
        "compose": side.compose,
        "hops": side.hops,
        "callers": side.callers,
        "tree": side.tree,
        "docs": side.docs,
        "budget": budget_page.budget,
        "adaptive": span.adaptive.map(|(k, t, g)| json!({ "kept": k, "total": t, "gap": g }))
    })
}
