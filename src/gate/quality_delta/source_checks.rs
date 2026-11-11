use super::clones::{collect_changed_clone_regressions, member_status};
use super::model::{QualityDeltaFacts, QualityRegression};
use crate::domain::{IngestResult, Interner};
use crate::gate::baseline::{QualitySnapshot, symbol_canonical_key};
use crate::gate::rename::{RenameMap, RenamePair};
use crate::query::commands::QueryContext;

fn matches_path(path: &str, file: &str) -> bool {
    path == file || path.ends_with(file)
}

pub fn symbol_matches_and_changed(
    sym: &crate::domain::Symbol,
    file_path: &str,
    ingest: &IngestResult,
    interner: &Interner,
    baseline: &QualitySnapshot,
) -> Option<(String, String)> {
    let sym_file = interner.resolve(&ingest.files[sym.file_id as usize].path);
    if !matches_path(sym_file, file_path) {
        return None;
    }
    let sym_name = interner.resolve(&sym.name);
    let key = match sym.scope {
        Some(sc) => format!("{sym_file}::{}::{sym_name}", interner.resolve(&sc)),
        None => format!("{sym_file}::{sym_name}"),
    };
    let &was_ccx = baseline.ccx_by_sym.get(&key)?;
    let was_loc = baseline.loc_by_sym.get(&key).copied().unwrap_or(0);
    if sym.ccx != was_ccx || sym.loc != was_loc {
        Some((sym_name.to_string(), sym_file.to_string()))
    } else {
        None
    }
}

fn find_preexisting_callables_in_file<'a>(
    file_path: &str,
    state: &'a QueryContext<'_>,
    baseline: &QualitySnapshot,
) -> Vec<(&'a crate::domain::Symbol, String, String)> {
    state
        .ingest
        .symbols
        .iter()
        .filter(|sym| {
            sym.kind == crate::domain::SymKind::Function
                || sym.kind == crate::domain::SymKind::Method
        })
        .filter_map(|sym| {
            symbol_matches_and_changed(sym, file_path, state.ingest, state.interner, baseline)
                .map(|(sym_name, sym_file)| (sym, sym_name, sym_file))
        })
        .collect()
}

pub(super) fn check_churn_regressions(
    state: &QueryContext<'_>,
    baseline: &QualitySnapshot,
    changed_files: &[String],
    regressions: &mut Vec<QualityRegression>,
) {
    let Some(commits) = &state.commits else {
        return;
    };
    if commits.is_empty() {
        return;
    }

    for f in changed_files {
        let commit_count = commits
            .iter()
            .filter(|c| c.changed_files.iter().any(|cf| cf == f || cf.ends_with(f)))
            .count();
        if commit_count < 2 {
            continue;
        }
        for (sym, sym_name, sym_file) in find_preexisting_callables_in_file(f, state, baseline) {
            regressions.push(QualityRegression {
                kind: "short-horizon-churn".to_string(),
                sym_name,
                file_path: sym_file,
                line: sym.line,
                was_val: 0,
                now_val: commit_count as u32,
                is_gating: false,
                is_new_symbol: false,
                renamed_from: None,
                ..QualityRegression::default()
            });
        }
    }
}

/// Body-text regression kinds read from code (and, for placeholder markers, comments) only.
#[must_use]
pub fn body_text_kinds(masked: &crate::metrics::source_mask::MaskedSource) -> Vec<&'static str> {
    let code = &masked.code;
    let compact: String = code.chars().filter(|c| !c.is_whitespace()).collect();
    let mut kinds = Vec::new();
    let code_placeholder = ["todo!(", "unimplemented!(", "NotImplementedError"]
        .iter()
        .any(|m| code.contains(m));
    let comment_placeholder = ["TODO:", "FIXME:"]
        .iter()
        .any(|m| masked.comments.contains(m));
    if code_placeholder || comment_placeholder {
        kinds.push("placeholder");
    }
    if [
        "catch{}",
        "catch(_){}",
        "catch(...){}",
        "except:pass",
        ".catch(()=>{})",
    ]
    .iter()
    .any(|m| compact.contains(m))
    {
        kinds.push("error-masking");
    }
    if code.contains("std::unique(") && !code.contains("std::sort(") {
        kinds.push("defect-shape");
    }
    kinds
}

fn check_body_text_regressions(
    sym: &crate::domain::Symbol,
    sym_name: &str,
    file_path: &str,
    body: &str,
    is_new: bool,
    pair: Option<&RenamePair>,
    regressions: &mut Vec<QualityRegression>,
) {
    let masked = crate::metrics::source_mask::mask_source(body, sym.lang);
    for kind in body_text_kinds(&masked) {
        if pair.is_some_and(|p| p.old_text_kinds.contains(&kind)) {
            continue;
        }
        regressions.push(QualityRegression {
            kind: kind.to_string(),
            sym_name: sym_name.to_string(),
            file_path: file_path.to_string(),
            line: sym.line,
            was_val: 0,
            now_val: 1,
            is_gating: false,
            is_new_symbol: is_new,
            renamed_from: pair.map(|p| p.old_name.clone()),
            ..QualityRegression::default()
        });
    }
}

pub fn is_changed_callable(
    sym: &crate::domain::Symbol,
    file_path: &str,
    changed_files: &[String],
) -> bool {
    (sym.kind == crate::domain::SymKind::Function || sym.kind == crate::domain::SymKind::Method)
        && changed_files
            .iter()
            .any(|cf| file_path == cf || file_path.ends_with(cf))
}

pub(super) fn check_source_and_clone_regressions(
    state: &QueryContext<'_>,
    facts: &QualityDeltaFacts<'_>,
    renames: &RenameMap,
    regressions: &mut Vec<QualityRegression>,
) {
    use crate::domain::CorpusView;
    let (baseline, changed_files) = (facts.baseline, facts.changed_files);
    for sym in &state.ingest.symbols {
        let file_path = state.sym_path(sym);
        if is_changed_callable(sym, file_path, changed_files)
            && let Ok(bytes) = state.sources.read(file_path)
            && !sym.decl_bytes(&bytes).is_empty()
        {
            let body = String::from_utf8_lossy(sym.decl_bytes(&bytes));
            let sym_name = state.sym_name(sym);
            let key = format!("{file_path}::{sym_name}");
            let scope = sym.scope.map(|sc| state.interner.resolve(&sc));
            let pair = renames.pair_for(&symbol_canonical_key(sym_name, file_path, scope));
            let is_new = pair.is_none() && !baseline.ccx_by_sym.contains_key(&key);
            check_body_text_regressions(sym, sym_name, file_path, &body, is_new, pair, regressions);
        }
    }
    let status = |sym: &crate::domain::Symbol| member_status(sym, state, facts, renames);
    collect_changed_clone_regressions(facts.clones, state, &status, regressions);
}

/// Drops dead-code rows whose name appears as an identifier elsewhere in the tree: a macro argument,
/// a table entry, a value or a re-export reaches it without a call edge. Where a local definition
/// shadows a bare name, a file that defines the name itself does not count.
pub(super) fn drop_referenced_dead_rows(
    state: &QueryContext<'_>,
    regressions: &mut Vec<QualityRegression>,
) {
    use std::collections::{HashMap, HashSet};
    let file_of: HashMap<&str, (crate::domain::Lang, u32)> = state
        .ingest
        .files
        .iter()
        .map(|f| (state.interner.resolve(&f.path), (f.lang, f.id)))
        .collect();
    let mut names: HashMap<crate::domain::Lang, HashSet<&str>> = HashMap::new();
    for r in regressions.iter().filter(|r| r.kind == "dead-code") {
        if let Some(&(lang, _)) = file_of.get(r.file_path.as_str()) {
            names
                .entry(lang)
                .or_default()
                .insert(r.sym_name.rsplit("::").next().unwrap_or(&r.sym_name));
        }
    }
    let uses: HashMap<crate::domain::Lang, HashMap<String, Vec<u32>>> = names
        .iter()
        .map(|(&lang, list)| {
            let found = crate::ingest::identifiers::identifier_use_files(
                state.ingest,
                state.interner,
                state.sources,
                lang,
                list,
            );
            (lang, found)
        })
        .collect();
    let scope = crate::gate::baseline::NameScope::of(state.ingest, state.graph, state.interner);
    let referenced = |r: &QualityRegression| -> bool {
        let Some(&(lang, file)) = file_of.get(r.file_path.as_str()) else {
            return false;
        };
        let name = r.sym_name.rsplit("::").next().unwrap_or(&r.sym_name);
        let Some(users) = uses.get(&lang).and_then(|found| found.get(name)) else {
            return false;
        };
        if !lang.semantics().local_definition_shadows_name() {
            return true;
        }
        let Some(spur) = state.interner.get(name) else {
            return true;
        };
        let method = r.sym_name.matches("::").count() > 1;
        users
            .iter()
            .any(|&user| scope.reaches(user, file, spur, method))
    };
    regressions.retain(|r| r.kind != "dead-code" || !referenced(r));
}
