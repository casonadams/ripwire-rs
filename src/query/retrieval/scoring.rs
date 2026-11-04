use super::ranking::is_module_anchor;
use super::terms::{matches_term_or_stem, normalize_camel_case_lower, stem_query_term};
use crate::domain::{IngestResult, Interner, SymKind};
use crate::query::lexical::Lexical;

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

/// Down-weight for symbols in test and bench files unless the query asks about tests.
fn test_factor(file_path: &str, name: &str, terms: &[&str]) -> f64 {
    let wants_test = terms.iter().any(|t| {
        let l = t.to_ascii_lowercase();
        l.contains("test") || l.contains("spec")
    });
    let is_test = crate::gate::affected::is_test_or_bench_path(file_path)
        || name.to_ascii_lowercase().starts_with("test_");
    if is_test && !wants_test { 0.05 } else { 1.0 }
}

/// Score of a symbol for a one-word lookup of an existing name: how well its name, scope or path says
/// the word. A path or scope hit alone (6 or less) is not enough to answer a lookup.
fn score_exact_name(
    sym: &crate::domain::Symbol,
    view: (&IngestResult, &Interner),
    term: &str,
) -> f64 {
    use crate::domain::CorpusView;
    let name = view.sym_name(sym);
    let (base, _) = score_single_term(
        &name.to_ascii_lowercase(),
        &normalize_camel_case_lower(name),
        view.sym_scope(sym)
            .map(normalize_camel_case_lower)
            .as_deref(),
        &view.sym_path(sym).to_ascii_lowercase(),
        term,
    );
    if base <= 6.0 { 0.0 } else { base }
}

/// Per-query inputs shared by every candidate's score.
pub(super) struct ScoringInputs<'a> {
    pub(super) ranks: &'a [f64],
    /// BM25 over the corpus for a conceptual query; `None` for a one-word lookup of an existing name.
    pub(super) lexical: Option<&'a Lexical>,
    pub(super) raw_terms: &'a [&'a str],
}

fn score_candidate_symbol(
    sym: &crate::domain::Symbol,
    ingest: &IngestResult,
    interner: &Interner,
    inputs: &ScoringInputs<'_>,
) -> f64 {
    use crate::domain::CorpusView;
    let conceptual = inputs.lexical.is_some();
    if !sym.is_code_symbol() && !(!conceptual && is_module_anchor(sym)) {
        return 0.0;
    }
    let view = (ingest, interner);
    let kind_factor = if matches!(sym.kind, SymKind::Var | SymKind::Field) {
        0.2
    } else if conceptual && sym.is_callable() && !sym.has_body() {
        // A bodyless declaration names the contract, not the coordinating implementation.
        0.5 * sym.lang.semantics().retrieval_weight()
    } else if conceptual && sym.kind == SymKind::Interface {
        // An interface is the contract that implementations satisfy; they coordinate the behavior.
        0.5 * sym.lang.semantics().retrieval_weight()
    } else {
        sym.lang.semantics().retrieval_weight()
    };
    let matched = match (inputs.lexical, inputs.raw_terms) {
        (Some(lexical), _) => lexical.score(sym),
        (None, [term]) => {
            let rank = inputs.ranks.get(sym.id as usize).copied().unwrap_or(0.0);
            score_exact_name(sym, view, term) * (1.0 + rank * 10.0)
        }
        (None, _) => 0.0,
    };
    let tests = test_factor(view.sym_path(sym), view.sym_name(sym), inputs.raw_terms);
    matched * tests * kind_factor
}

/// Every in-scope symbol with a positive score for the query, unsorted.
pub(super) fn score_candidates<'a>(
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
