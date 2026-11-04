use super::terms::{contains_term_aware, stem_query_term};
use crate::domain::{IngestResult, Interner, SymKind};
use crate::query::lexical::{Lexical, mentions};

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

pub(super) fn compute_for_route_metadata(
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

/// Percent of query terms that a top row names, or, for a conceptual query, whose document mentions.
pub(super) fn term_coverage_percent(
    raw_terms: &[&str],
    scored: &[(f64, &crate::domain::Symbol)],
    lexical: Option<&Lexical>,
    interner: &Interner,
) -> u64 {
    if raw_terms.is_empty() {
        return 100;
    }
    let top = || scored.iter().take(10).map(|(_, s)| *s);
    let documents: Vec<_> = lexical
        .map(|lex| top().map(|s| lex.tokens(s)).collect())
        .unwrap_or_default();
    let covered = raw_terms
        .iter()
        .filter(|&&t| {
            let lower_t = t.to_ascii_lowercase();
            top().any(|s| {
                interner
                    .resolve(&s.name)
                    .to_ascii_lowercase()
                    .contains(&lower_t)
            }) || documents.iter().any(|tokens| mentions(tokens, t))
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

/// True for a one-word query naming an existing symbol; such lookups keep the exact-name scoring.
pub(super) fn is_exact_name_query(
    ingest: &IngestResult,
    interner: &Interner,
    raw_terms: &[&str],
) -> bool {
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
pub(super) fn is_module_anchor(sym: &crate::domain::Symbol) -> bool {
    sym.kind == SymKind::Section && sym.lang.semantics().module_declarations_are_anchors()
}

fn is_name_anchor(sym: &crate::domain::Symbol) -> bool {
    sym.is_code_symbol() || is_module_anchor(sym)
}

/// Stable promotion of the symbols a one-word query names exactly (variables and fields excluded), so the
/// named declaration leads the ranking; the rest keep their score order.
pub(super) fn promote_exact_anchors(
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

pub(super) fn diversify_top_candidates<'a>(
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
