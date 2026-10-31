pub use super::grep::*;
pub use super::search::*;

use super::evidence::{SymbolEvidence, TermEvidence};
use crate::domain::{IngestResult, Interner, NodeId, SymKind};
use crate::graph::Graph;
use serde_json::{Value, json};

/// Most rows the default body posture serves inline (the exact-name definitions, in rank order).
const AUTO_BODY_ROWS: usize = 6;

fn stem_derivational_suffix(t: &str) -> Option<String> {
    if let Some(prefix) = t.strip_suffix("ution").filter(|p| p.len() >= 3) {
        return Some(format!("{prefix}v"));
    }
    for (suffix, min_stem) in [
        ("ization", 4),
        ("ation", 4),
        ("ition", 4),
        ("tion", 4),
        ("sion", 4),
        ("ment", 4),
        ("ing", 4),
        ("y", 5),
    ] {
        if let Some(stripped) = t.strip_suffix(suffix)
            && stripped.len() >= min_stem
        {
            return Some(stripped.to_string());
        }
    }
    None
}

pub(super) fn stem_plural_only(term: &str) -> String {
    let mut t = term.to_ascii_lowercase();
    if t.len() >= 5 && t.ends_with('s') && !t.ends_with("ss") {
        t.pop();
    }
    t
}

fn stem_query_term(term: &str) -> String {
    let t = stem_plural_only(term);
    stem_derivational_suffix(&t).unwrap_or(t)
}

fn is_camel_boundary(bytes: &[u8], i: usize) -> bool {
    if i == 0 || !bytes[i].is_ascii_uppercase() {
        return false;
    }
    let prev_lower = bytes[i - 1].is_ascii_lowercase() || bytes[i - 1].is_ascii_digit();
    let next_lower =
        bytes.get(i + 1).is_some_and(u8::is_ascii_lowercase) && bytes[i - 1].is_ascii_uppercase();
    prev_lower || next_lower
}

fn normalize_camel_case_lower(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = String::with_capacity(s.len() + 4);
    for (i, ch) in s.char_indices() {
        if is_camel_boundary(bytes, i) {
            out.push('_');
        }
        out.push(ch.to_ascii_lowercase());
    }
    out
}

fn is_for_stopword(w: &str) -> bool {
    matches!(
        w.to_ascii_lowercase().as_str(),
        "how"
            | "the"
            | "and"
            | "are"
            | "for"
            | "with"
            | "from"
            | "into"
            | "what"
            | "when"
            | "where"
            | "which"
            | "that"
            | "this"
            | "does"
            | "can"
            | "via"
            | "use"
            | "used"
            | "using"
    )
}

fn contains_term_aware(haystack: &str, needle: &str) -> bool {
    if needle.len() >= 4 {
        return haystack.contains(needle);
    }
    haystack
        .split(|c: char| !c.is_ascii_alphanumeric())
        .any(|tok| tok == needle || (needle.len() == 3 && tok.starts_with(needle)))
}

/// Score a symbol against a single search term across name, scope, and file path.
fn matches_term_or_stem(haystack: &str, t: &str, stem: &str) -> bool {
    contains_term_aware(haystack, t) || contains_term_aware(haystack, stem)
}

fn score_single_term(
    lower_name: &str,
    norm_name: &str,
    lower_scope: Option<&str>,
    lower_path: &str,
    term: &str,
) -> (f64, bool) {
    let t = term.to_ascii_lowercase();
    let stem = stem_query_term(&t);
    let name_hit = if lower_name == t || lower_name == stem {
        30.0
    } else if matches_term_or_stem(lower_name, &t, &stem)
        || matches_term_or_stem(norm_name, &t, &stem)
    {
        12.0
    } else {
        0.0
    };
    let scope_hit = if lower_scope.is_some_and(|sc| matches_term_or_stem(sc, &t, &stem)) {
        6.0
    } else {
        0.0
    };
    let path_hit = if !lower_path.is_empty() && matches_term_or_stem(lower_path, &t, &stem) {
        4.0
    } else {
        0.0
    };
    (
        name_hit + scope_hit + path_hit,
        name_hit > 0.0 || scope_hit > 0.0,
    )
}

fn compute_term_idf_weights(
    ingest: &IngestResult,
    interner: &Interner,
    raw_terms: &[&str],
) -> Vec<f64> {
    if raw_terms.is_empty() {
        return Vec::new();
    }
    let total = ingest
        .symbols
        .iter()
        .filter(|s| s.is_code_symbol())
        .count()
        .max(1) as f64;
    raw_terms
        .iter()
        .map(|&term| {
            let df = ingest
                .symbols
                .iter()
                .filter(|s| s.is_code_symbol() && symbol_matches_term(s, ingest, interner, term))
                .count() as f64;
            1.0 + ((total / (df + 1.0)).ln().max(0.0) * 0.5)
        })
        .collect()
}

struct SymbolMatchCtx<'a> {
    name: &'a str,
    scope: Option<&'a str>,
    file_path: &'a str,
    evidence: SymbolEvidence,
    terms: &'a [&'a str],
    idf_weights: &'a [f64],
    rank: f64,
}

fn compute_match_synergy(direct_hits: usize, body_only_hits: usize, name_hits: usize) -> f64 {
    let eff_matches = direct_hits as f64 + (body_only_hits as f64) * 0.7;
    eff_matches.powi(3) * (1.0 + (name_hits.saturating_sub(1) as f64) * 0.75)
}

fn score_symbol_match_with_idf(ctx: &SymbolMatchCtx<'_>) -> f64 {
    let lower_name = ctx.name.to_ascii_lowercase();
    let norm_name = normalize_camel_case_lower(ctx.name);
    let lower_scope = ctx.scope.map(normalize_camel_case_lower);
    let lower_path = ctx.file_path.to_ascii_lowercase();
    let mut direct_hits = 0usize;
    let mut body_only_hits = 0usize;
    let mut name_hits = 0usize;
    let mut max_raw = 0.0f64;
    let mut weighted_sum = 0.0f64;
    for (i, &t) in ctx.terms.iter().enumerate() {
        let (base, is_name_hit) = score_single_term(
            &lower_name,
            &norm_name,
            lower_scope.as_deref(),
            &lower_path,
            t,
        );
        name_hits += usize::from(is_name_hit);
        let raw = base + ctx.evidence.term_weight(i);
        if raw > 0.0 {
            direct_hits += usize::from(base > 0.0);
            body_only_hits += usize::from(base == 0.0);
            max_raw = max_raw.max(raw);
            let idf = ctx.idf_weights.get(i).copied().unwrap_or(1.0);
            weighted_sum += raw * idf;
        }
    }
    // A lone body-only term is a weak hint; two distinct query concepts called from one body coordinate them.
    if max_raw <= 6.0 && body_only_hits < 2 {
        return 0.0;
    }
    let synergy = compute_match_synergy(direct_hits, body_only_hits, name_hits);
    let query_wants_test = ctx.terms.iter().any(|t| {
        let l = t.to_ascii_lowercase();
        l.contains("test") || l.contains("spec")
    });
    let is_test = crate::gate::affected::is_test_or_bench_path(ctx.file_path)
        || lower_name.starts_with("test_");
    let non_test = if is_test && !query_wants_test {
        0.05
    } else {
        1.0
    };
    weighted_sum * synergy * non_test * (1.0 + ctx.rank * 10.0)
}

/// Per-query inputs shared by every candidate's score.
struct ScoringInputs<'a> {
    ranks: &'a [f64],
    evidence: &'a TermEvidence,
    raw_terms: &'a [&'a str],
    idf_weights: &'a [f64],
    conceptual: bool,
}

fn score_candidate_symbol(
    sym: &crate::domain::Symbol,
    ingest: &IngestResult,
    interner: &Interner,
    inputs: &ScoringInputs<'_>,
) -> f64 {
    use crate::domain::CorpusView;
    if !sym.is_code_symbol() && !(!inputs.conceptual && is_module_anchor(sym)) {
        return 0.0;
    }
    let view = (ingest, interner);
    let file_path = view.sym_path(sym);
    let kind_factor = if matches!(sym.kind, SymKind::Var | SymKind::Field) {
        0.2
    } else if inputs.conceptual && sym.is_callable() && !sym.has_body() {
        // A bodyless declaration names the contract, not the coordinating implementation.
        0.5 * sym.lang.semantics().retrieval_weight()
    } else if inputs.conceptual && sym.kind == SymKind::Interface {
        // An interface is the contract that implementations satisfy; they coordinate the behavior.
        0.5 * sym.lang.semantics().retrieval_weight()
    } else {
        sym.lang.semantics().retrieval_weight()
    };
    let ctx = SymbolMatchCtx {
        name: view.sym_name(sym),
        scope: view.sym_scope(sym),
        file_path,
        evidence: inputs.evidence.of(sym),
        terms: inputs.raw_terms,
        idf_weights: inputs.idf_weights,
        rank: inputs.ranks.get(sym.id as usize).copied().unwrap_or(0.0),
    };
    score_symbol_match_with_idf(&ctx) * kind_factor
}

fn extract_normalized_sig(
    sym: &crate::domain::Symbol,
    bytes: &[u8],
    fallback_name: &str,
) -> String {
    sym.extract_signature(bytes, fallback_name)
}

fn read_file_cached<'a, 's>(
    sources: &'s dyn crate::query::SourceReader,
    rel_path: &str,
    file_id: u32,
    cache: &'a mut std::collections::HashMap<u32, std::borrow::Cow<'s, [u8]>>,
) -> &'a [u8] {
    cache
        .entry(file_id)
        .or_insert_with(|| sources.read(rel_path).unwrap_or_default())
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
/// Execute `for` task-lens ranking tool.
pub fn compute_adaptive_cut(scores: &[f64], floor: usize, ceiling: usize) -> (usize, usize, f64) {
    let limit = ceiling.min(scores.len());
    if limit <= floor {
        return (limit, limit, 0.0);
    }
    let mut max_gap = 0.0;
    let mut best_k = limit;
    for k in floor..limit {
        let prev = scores[k - 1];
        let curr = scores[k];
        if prev > 0.0 {
            let gap = (prev - curr) / prev;
            if gap > max_gap {
                max_gap = gap;
                best_k = k;
            }
        }
    }
    if max_gap >= 0.25 {
        (best_k, best_k, max_gap)
    } else {
        (limit, limit, max_gap)
    }
}

/// Longest signature a ranked row shows. A constant's signature is its whole initializer, which would
/// otherwise take the budget of a dozen rows; `--expand` serves the rest.
const MAX_SIGNATURE_BYTES: usize = 240;

fn cap_signature(mut sig: String) -> String {
    if sig.len() > MAX_SIGNATURE_BYTES {
        let mut cut = MAX_SIGNATURE_BYTES;
        while !sig.is_char_boundary(cut) {
            cut -= 1;
        }
        sig.truncate(cut);
        sig.push('…');
    }
    sig
}

/// Bytes one ranked symbol row costs: its name, file path and signature span.
fn symbol_row_bytes(
    sym: &crate::domain::Symbol,
    ingest: &IngestResult,
    interner: &Interner,
) -> usize {
    let signature =
        (sym.sig_end_byte.saturating_sub(sym.sig_start_byte) as usize).min(MAX_SIGNATURE_BYTES);
    crate::serialize::text_bytes(&[
        interner.resolve(&sym.name),
        ingest.sym_file_path(sym, interner),
    ]) + signature
}

/// Rows that the body policy asks to serve inline. Explicit `--detail` and `--auto-bodies` take the first N
/// rows. The default posture serves only the exact-name definitions on the name-exact route.
fn body_wanted(idx: usize, name: &str, opts: ForTaskOptions<'_>, exact_term: Option<&str>) -> bool {
    if opts.signatures_only {
        return false;
    }
    match opts.detail.or(opts.auto_bodies.then_some(AUTO_BODY_ROWS)) {
        Some(count) => idx < count,
        None => {
            idx < AUTO_BODY_ROWS && exact_term.is_some_and(|term| term.eq_ignore_ascii_case(name))
        }
    }
}

/// One ranked row. `wants_body` is the body policy's ask; the budget decides whether the body is served.
struct RankedRow<'a> {
    score: f64,
    sym: &'a crate::domain::Symbol,
    is_tested: bool,
    wants_body: bool,
}

fn build_for_symbol_entry<'s>(
    row: &RankedRow<'_>,
    view: &dyn crate::domain::CorpusView<'_>,
    sources: Option<&'s dyn crate::query::SourceReader>,
    graph: Option<&Graph>,
    file_cache: &mut std::collections::HashMap<u32, std::borrow::Cow<'s, [u8]>>,
    body_room: &mut usize,
) -> Value {
    let sym = row.sym;
    let name = view.sym_name(sym);
    let path = view.sym_path(sym);
    let bytes = sources
        .map(|sources| read_file_cached(sources, path, sym.file_id, file_cache))
        .unwrap_or_default();
    let sig = cap_signature(extract_normalized_sig(sym, bytes, name));
    let in_deg = graph.map_or(0, |g| g.in_degree(sym.id));
    let end_line = sym.line.saturating_add(sym.loc.saturating_sub(1));
    let mut obj = json!({
        "name": name,
        "kind": sym.kind.tag(),
        "path": path,
        "line": sym.line,
        "end_line": end_line,
        "sig": sig,
        "cx": sym.cx,
        "ccx": sym.ccx,
        "in": in_deg,
        "amp": in_deg.max(1),
        "tested": u8::from(row.is_tested),
        "score": (row.score * 100.0).round() / 100.0
    });
    if row.wants_body
        && let Some(body) = sym.source_text(bytes)
        && body.len() <= *body_room
    {
        *body_room -= body.len();
        obj["body"] = Value::String(body.into_owned());
    }
    obj
}

/// The bundle kind, why bodies were left out (if they were), and an executable `--expand` for the first row without one.
fn body_disclosure(
    opts: ForTaskOptions<'_>,
    conceptual: bool,
    rows: &[Value],
    wanted: usize,
    served: usize,
) -> (&'static str, Option<&'static str>, Option<String>) {
    let bundle = if conceptual { "compact" } else { "auto" };
    let reason = if rows.is_empty() {
        None
    } else if opts.signatures_only {
        Some("signatures-only")
    } else if served < wanted {
        Some("budget")
    } else if served == 0 && conceptual && opts.detail.is_none() && !opts.auto_bodies {
        Some("compact-route")
    } else {
        None
    };
    let follow = reason
        .and(rows.iter().find(|row| !row["body"].is_string()))
        .map(|row| {
            format!(
                "--expand={}:{}",
                row["path"].as_str().unwrap_or_default(),
                row["name"].as_str().unwrap_or_default()
            )
        });
    (bundle, reason, follow)
}

fn compute_for_route_metadata(
    task: &str,
    raw_terms: &[&str],
    scored: &[(f64, &crate::domain::Symbol)],
    interner: &Interner,
) -> (String, &'static str, u64) {
    let exact_hit = raw_terms.len() == 1
        && scored
            .first()
            .is_some_and(|(_, s)| interner.resolve(&s.name).eq_ignore_ascii_case(raw_terms[0]));
    let route = if exact_hit {
        format!("name-exact({task})")
    } else if raw_terms.len() <= 2 && !scored.is_empty() {
        "subtoken+body:focused".to_string()
    } else {
        "subtoken+body:broad".to_string()
    };
    let margin_pct = if scored.len() >= 2 && scored[0].0 > 0.0 {
        (((scored[0].0 - scored[1].0) / scored[0].0) * 100.0).round() as u64
    } else {
        u64::from(scored.len() == 1) * 100
    };
    let confidence = if exact_hit || margin_pct >= 25 {
        "high"
    } else if !scored.is_empty() {
        "medium"
    } else {
        "low"
    };
    (route, confidence, margin_pct)
}

/// Percent of query terms that a top row names, or that its evidence mentions when `evidence` is given.
fn term_coverage_percent(
    raw_terms: &[&str],
    scored: &[(f64, &crate::domain::Symbol)],
    evidence: Option<&TermEvidence>,
    interner: &Interner,
) -> u64 {
    if raw_terms.is_empty() {
        return 100;
    }
    let covered = raw_terms
        .iter()
        .enumerate()
        .filter(|&(i, &t)| {
            let lower_t = t.to_ascii_lowercase();
            scored.iter().take(10).any(|(_, s)| {
                interner
                    .resolve(&s.name)
                    .to_ascii_lowercase()
                    .contains(&lower_t)
                    || evidence.is_some_and(|e| e.of(s).mentions(i))
            })
        })
        .count();
    ((covered * 100) / raw_terms.len()) as u64
}

fn symbol_matches_term(
    sym: &crate::domain::Symbol,
    ingest: &IngestResult,
    interner: &Interner,
    term: &str,
) -> bool {
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let lower_t = term.to_ascii_lowercase();
    let stem = stem_query_term(&lower_t);
    let name = view.sym_name(sym).to_ascii_lowercase();
    let path = view.sym_path(sym).to_ascii_lowercase();
    contains_term_aware(&name, &lower_t)
        || contains_term_aware(&name, &stem)
        || contains_term_aware(&path, &lower_t)
        || contains_term_aware(&path, &stem)
}

/// Every in-scope symbol with a positive score for the query, unsorted.
fn score_candidates<'a>(
    ingest: &'a IngestResult,
    interner: &Interner,
    inputs: &ScoringInputs<'_>,
    in_scope: Option<&str>,
) -> Vec<(f64, &'a crate::domain::Symbol)> {
    let mut scored = Vec::new();
    for sym in &ingest.symbols {
        if in_scope.is_some_and(|scope| !ingest.sym_file_path(sym, interner).contains(scope)) {
            continue;
        }
        let score = score_candidate_symbol(sym, ingest, interner, inputs);
        if score > 0.0 {
            scored.push((score, sym));
        }
    }
    scored
}

/// True for a one-word query naming an existing symbol; such lookups keep the exact-name scoring.
fn is_exact_name_query(ingest: &IngestResult, interner: &Interner, raw_terms: &[&str]) -> bool {
    use crate::domain::CorpusView;
    let [term] = raw_terms else {
        return false;
    };
    let view = (ingest, interner);
    ingest
        .symbols
        .iter()
        .any(|s| is_name_anchor(s) && view.sym_name(s).eq_ignore_ascii_case(term))
}

/// A declaration an exact-name query can anchor on: a language's module declaration (`mod name;`).
fn is_module_anchor(sym: &crate::domain::Symbol) -> bool {
    sym.kind == SymKind::Section && sym.lang.semantics().module_declarations_are_anchors()
}

fn is_name_anchor(sym: &crate::domain::Symbol) -> bool {
    sym.is_code_symbol() || is_module_anchor(sym)
}

/// Stable promotion of the symbols a one-word query names exactly (variables and fields excluded), so the
/// named declaration leads the ranking; the rest keep their score order.
fn promote_exact_anchors(
    scored: &mut [(f64, &crate::domain::Symbol)],
    term: &str,
    interner: &Interner,
) {
    let is_anchor = |s: &crate::domain::Symbol| {
        !matches!(s.kind, SymKind::Var | SymKind::Field)
            && interner.resolve(&s.name).eq_ignore_ascii_case(term)
    };
    scored.sort_by_key(|(_, s)| !is_anchor(s));
}

fn diversify_top_candidates<'a>(
    scored: &mut [(f64, &'a crate::domain::Symbol)],
    raw_terms: &[&str],
    ingest: &'a IngestResult,
    interner: &'a Interner,
) {
    if raw_terms.len() < 3 || scored.len() < 6 {
        return;
    }
    let mut insert_pos = 2usize;
    for &term in raw_terms.iter().take(8) {
        if insert_pos >= 8 || insert_pos >= scored.len() {
            break;
        }
        let covered = scored[..insert_pos + 2]
            .iter()
            .any(|(_, s)| symbol_matches_term(s, ingest, interner, term));
        if covered {
            continue;
        }
        if let Some(rel_idx) = scored[insert_pos..].iter().take(60).position(|(_, s)| {
            !matches!(s.kind, SymKind::Var | SymKind::Field)
                && symbol_matches_term(s, ingest, interner, term)
        }) {
            let found_idx = insert_pos + rel_idx;
            scored[insert_pos..=found_idx].rotate_right(1);
            insert_pos += 1;
        }
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
    let raw_terms = if filtered_terms.is_empty() {
        all_terms
    } else {
        filtered_terms
    };

    let conceptual = !is_exact_name_query(ingest, interner, &raw_terms);
    let mut evidence = TermEvidence::from_calls(ingest, interner, &raw_terms);
    if let (true, Some(sources)) = (conceptual, opts.sources) {
        evidence.add_text(ingest, interner, sources, &raw_terms);
    }
    let idf_weights = compute_term_idf_weights(ingest, interner, &raw_terms);
    let inputs = ScoringInputs {
        ranks,
        evidence: &evidence,
        raw_terms: &raw_terms,
        idf_weights: &idf_weights,
        conceptual,
    };
    let mut scored = score_candidates(ingest, interner, &inputs, opts.in_scope);

    scored.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));
    diversify_top_candidates(&mut scored, &raw_terms, ingest, interner);
    if let [term] = raw_terms.as_slice() {
        promote_exact_anchors(&mut scored, term, interner);
    }
    let budget_page = crate::serialize::PageRequest {
        limit: None,
        offset: None,
        budget: opts
            .budget_tokens
            .map(crate::query::map::token_budget_bytes),
    };
    let window = crate::serialize::PageRequest {
        limit: opts.limit,
        offset: opts.offset,
        budget: budget_page.budget,
    }
    .window_fit(scored.len(), |i| {
        symbol_row_bytes(scored[i].1, ingest, interner)
    });
    let begin = window.begin;
    let mut end = window.end;
    let mut adaptive_info = None;
    if opts.adaptive && window.shown > 5 {
        let scores: Vec<f64> = scored[begin..end].iter().map(|s| s.0).collect();
        let (kept, _cliff, gap) = compute_adaptive_cut(&scores, 5, scores.len());
        adaptive_info = Some((kept, scored.len(), gap));
        end = begin + kept;
    }
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let exact_term = match raw_terms.as_slice() {
        [term] if !conceptual => Some(*term),
        _ => None,
    };
    let wants: Vec<bool> = scored[begin..end]
        .iter()
        .enumerate()
        .map(|(idx, (_, sym))| body_wanted(begin + idx, view.sym_name(sym), opts, exact_term))
        .collect();
    // Default bodies are admitted into the budget left after the signature rows; explicit requests are not.
    let shown_bytes: usize = (begin..end)
        .map(|i| symbol_row_bytes(scored[i].1, ingest, interner))
        .sum();
    let mut body_room = if opts.detail.is_some() || opts.auto_bodies {
        usize::MAX
    } else {
        budget_page
            .budget
            .unwrap_or(crate::serialize::DEFAULT_PACK_BUDGET_BYTES)
            .saturating_sub(shown_bytes)
    };
    let mut file_cache = std::collections::HashMap::new();
    let tested_bitmap = opts.graph.map(|g| view.compute_test_reachability(g));
    let symbols_json: Vec<Value> = scored[begin..end]
        .iter()
        .enumerate()
        .map(|(idx, (score, sym))| {
            let is_tested = tested_bitmap
                .as_ref()
                .and_then(|bm| bm.get(sym.id as usize).copied())
                .unwrap_or_else(|| view.is_test_symbol(sym));
            let row = RankedRow {
                score: *score,
                sym,
                is_tested,
                wants_body: wants[idx],
            };
            build_for_symbol_entry(
                &row,
                &view,
                opts.sources,
                opts.graph,
                &mut file_cache,
                &mut body_room,
            )
        })
        .collect();
    let wanted = wants.iter().filter(|w| **w).count();
    let served = symbols_json
        .iter()
        .filter(|s| s["body"].is_string())
        .count();
    let (bundle, reason, follow) = body_disclosure(opts, conceptual, &symbols_json, wanted, served);
    // Exact-name lookups keep name-only coverage; conceptual rows also count their source text.
    let coverage_evidence = conceptual.then_some(&evidence);
    let (route, confidence, margin_pct) =
        compute_for_route_metadata(task, &raw_terms, &scored, interner);
    let coverage = term_coverage_percent(&raw_terms, &scored, coverage_evidence, interner);
    let window_rows = &scored[begin..end];
    let lego = collect_for_lego(ingest, interner, window_rows);
    let compose = collect_for_compose(ingest, interner, window_rows);
    let hops = collect_for_hops(ingest, interner, opts.graph, window_rows, budget_page);
    let callers = exact_term.and_then(|term| {
        collect_for_callers(ingest, interner, opts.graph, window_rows, term, ranks)
    });
    let tree = collect_for_tree_rows(ingest, interner, &scored, begin, end);
    let docs = collect_for_doc_mentions(ingest, interner, &raw_terms, budget_page);
    let sig_bytes: usize = symbols_json
        .iter()
        .map(|s| {
            s["sig"].as_str().map_or(0, str::len)
                + s["body"].as_str().map_or(0, str::len)
                + s["path"].as_str().map_or(0, str::len)
                + 64
        })
        .sum();
    let est_tokens = ((sig_bytes + 320) / 4).max(1) as u64;
    json!({
        "task": task,
        "root": opts.root,
        "at": opts.at,
        "route": route,
        "confidence": confidence,
        "margin_pct": margin_pct,
        "coverage": coverage,
        "est_tokens": est_tokens,
        "symbols": symbols_json,
        "count": symbols_json.len(),
        "window": {
            "begin": begin,
            "end": end,
            "total": scored.len(),
            "limit": opts.limit.unwrap_or(0),
            "offset": opts.offset.unwrap_or(0),
        },
        "detail": wanted,
        "bundle": bundle,
        "reason": reason,
        "follow": follow,
        "sections": sections,
        "lego": lego,
        "compose": compose,
        "hops": hops,
        "callers": callers,
        "tree": tree,
        "docs": docs,
        "budget": budget_page.budget,
        "adaptive": adaptive_info.map(|(k, t, g)| json!({ "kept": k, "total": t, "gap": g }))
    })
}

/// Most direct callers a name-exact `--for` lists; the rest stay behind `--callers`.
const FOR_CALLERS_CAP: usize = 8;

/// The direct callers of the one definition a name-exact `--for` answers: `of`, the `total`, and up to
/// `FOR_CALLERS_CAP` rows ranked by importance. `None` when the name binds several definitions, the graph
/// is absent, or nothing calls it (`in=` on the signature row already says so).
fn collect_for_callers(
    ingest: &IngestResult,
    interner: &Interner,
    graph: Option<&crate::graph::Graph>,
    window_rows: &[(f64, &crate::domain::Symbol)],
    term: &str,
    ranks: &[f64],
) -> Option<Value> {
    let [(_, sym)] = window_rows else {
        return None;
    };
    if interner.resolve(&sym.name) != term {
        return None;
    }
    let graph = graph?;
    let mut callers: Vec<(NodeId, bool)> = graph
        .callers(sym.id)
        .map(|(caller, _)| {
            let by_name = graph
                .edge_prov(caller, sym.id)
                .is_some_and(crate::resolve::EdgeProv::is_by_name);
            (caller, by_name)
        })
        .collect();
    if callers.is_empty() {
        return None;
    }
    let rank_of = |id: NodeId| ranks.get(id as usize).copied().unwrap_or(0.0);
    callers.sort_by(|a, b| {
        rank_of(b.0)
            .total_cmp(&rank_of(a.0))
            .then_with(|| a.0.cmp(&b.0))
    });
    let total = callers.len();
    let rows: Vec<Value> = callers
        .into_iter()
        .take(FOR_CALLERS_CAP)
        .filter_map(|(id, by_name)| {
            let caller = ingest.sym(id)?;
            Some(json!({
                "name": interner.resolve(&caller.name),
                "kind": caller.kind.tag(),
                "path": ingest.sym_file_path(caller, interner),
                "line": caller.line,
                "by_name": by_name,
            }))
        })
        .collect();
    Some(json!({ "of": term, "total": total, "rows": rows }))
}

/// Interfaces that are ranked, or declared in a file holding a ranked symbol, and have implementors.
fn collect_for_lego(
    ingest: &IngestResult,
    interner: &Interner,
    ranked: &[(f64, &crate::domain::Symbol)],
) -> Vec<Value> {
    let ranked_files: std::collections::HashSet<u32> =
        ranked.iter().map(|(_, s)| s.file_id).collect();
    let mut seen = std::collections::HashSet::new();
    let mut out = Vec::new();
    let index = std::cell::OnceCell::new();
    for sym in &ingest.symbols {
        if sym.kind != SymKind::Interface || !ranked_files.contains(&sym.file_id) {
            continue;
        }
        let name = interner.resolve(&sym.name);
        if !seen.insert(name) {
            continue;
        }
        let lego = call_lego_with(ingest, interner, name, &index);
        if lego["count"].as_u64().unwrap_or(0) == 0 {
            continue;
        }
        let path = ingest
            .files
            .get(sym.file_id as usize)
            .map_or("", |f| interner.resolve(&f.path));
        out.push(json!({
            "name": name,
            "path": path,
            "line": sym.line,
            "implementors": lego["implementors"]
        }));
    }
    out
}

fn collect_for_compose(
    ingest: &IngestResult,
    interner: &Interner,
    ranked: &[(f64, &crate::domain::Symbol)],
) -> Vec<Value> {
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let ranked_files: std::collections::HashSet<u32> =
        ranked.iter().map(|(_, s)| s.file_id).collect();
    let mut out = Vec::new();
    for sym in &ingest.symbols {
        if sym.kind != SymKind::Field || !ranked_files.contains(&sym.file_id) {
            continue;
        }
        let Some(owner) = view.sym_scope(sym) else {
            continue;
        };
        out.push(json!({
            "name": view.sym_name(sym),
            "owner": owner,
            "rel": "uses",
        }));
        if out.len() >= 32 {
            break;
        }
    }
    out
}

fn collect_for_hops(
    ingest: &IngestResult,
    interner: &Interner,
    graph_opt: Option<&Graph>,
    ranked: &[(f64, &crate::domain::Symbol)],
    page: crate::serialize::PageRequest,
) -> Vec<Value> {
    use crate::domain::CorpusView;
    let Some(graph) = graph_opt else {
        return Vec::new();
    };
    let view = (ingest, interner);
    let mut hops = Vec::new();
    for &(_, sym) in ranked {
        let callees: Vec<Value> = graph
            .callees(sym.id)
            .filter_map(|(cid, _)| ingest.sym(cid))
            .map(|csym| {
                json!({
                    "name": view.sym_name(csym),
                    "line": csym.line,
                })
            })
            .collect();
        if callees.is_empty() {
            continue;
        }
        let calls = crate::serialize::Paged::new(&callees, page, |c| c.to_string().len())
            .rows()
            .to_vec();
        hops.push(json!({
            "name": view.sym_name(sym),
            "path": view.sym_path(sym),
            "line": sym.line,
            "total": callees.len(),
            "calls": calls,
        }));
    }
    let window = crate::serialize::Paged::new(&hops, page, |h| h.to_string().len()).window;
    hops.into_iter()
        .skip(window.begin)
        .take(window.shown)
        .collect()
}

fn collect_for_tree_rows(
    ingest: &IngestResult,
    interner: &Interner,
    scored: &[(f64, &crate::domain::Symbol)],
    begin: usize,
    end: usize,
) -> Vec<Value> {
    use crate::domain::CorpusView;
    let view = (ingest, interner);
    let shown_files: std::collections::HashSet<u32> =
        scored[begin..end].iter().map(|(_, s)| s.file_id).collect();
    let mut seen_tree_files = std::collections::HashSet::new();
    let mut rows = Vec::new();
    for &(_, sym) in scored.iter().skip(end) {
        if rows.len() >= 12 {
            break;
        }
        if !shown_files.contains(&sym.file_id) && seen_tree_files.insert(sym.file_id) {
            rows.push(json!({
                "path": view.sym_path(sym),
                "sym": view.sym_name(sym),
                "kind": sym.kind.tag(),
            }));
        }
    }
    rows
}

fn collect_for_doc_mentions(
    ingest: &IngestResult,
    interner: &Interner,
    raw_terms: &[&str],
    page: crate::serialize::PageRequest,
) -> Vec<Value> {
    let mut doc_hits: std::collections::BTreeMap<&str, usize> = std::collections::BTreeMap::new();
    for sym in &ingest.symbols {
        if sym.kind != SymKind::Section || !sym.lang.semantics().is_prose_document() {
            continue;
        }
        let sec_lower = interner.resolve(&sym.name).to_ascii_lowercase();
        let hits = raw_terms
            .iter()
            .filter(|&&t| {
                let low = t.to_ascii_lowercase();
                contains_term_aware(&sec_lower, &low)
            })
            .count();
        if hits > 0 {
            let p = ingest.sym_file_path(sym, interner);
            *doc_hits.entry(p).or_default() += hits;
        }
    }
    let mut docs: Vec<(&str, usize)> = doc_hits.into_iter().collect();
    docs.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    let window =
        crate::serialize::Paged::new(&docs, page, |d| crate::serialize::text_bytes(&[d.0])).window;
    docs[window.begin..window.end]
        .iter()
        .map(|&(p, m)| json!({ "path": p, "mentions": m }))
        .collect()
}
