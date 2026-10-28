//! Rename-aware identity for the quality delta.
//!
//! A baseline callable that vanished and a working callable that appeared are the same entity under a new
//! name when they share a kind and parameter arity and their bodies match under the clone detector's own rule:
//! an identical normalized body, or Type-3 similarity at or above the clone floor. Pairing is one-to-one. A pair
//! forms only when each side's single best candidate is the other, so ties and ambiguous candidates stay
//! unpaired and read as a removed symbol plus a new one.

use crate::clones::detector::{
    BodyTokens, sketch_stage_passes, symbol_body_tokens, type3_similarity,
};
use crate::domain::{IngestResult, Interner, SymKind, Symbol};
use crate::gate::baseline::{QualitySnapshot, symbol_canonical_key};
use crate::gate::quality_delta::{body_text_kinds, is_changed_callable};
use crate::metrics::source_mask::mask_source;
use crate::query::SourceReader;
use std::borrow::Cow;
use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};

/// Identity-free shape of one callable, held in memory for the changed files only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BodyFingerprint {
    pub kind: SymKind,
    pub(crate) body: BodyTokens,
    /// Placeholder and error-masking markers in the declaration (see `body_text_kinds`).
    pub text_kinds: Vec<&'static str>,
}

impl BodyFingerprint {
    /// Whether two fingerprints hold the same body text, across separately built indexes whose node
    /// ids differ.
    #[must_use]
    pub fn same_text(&self, other: &Self) -> bool {
        self.body.raw_hash == other.body.raw_hash
    }
}

/// A baseline symbol the working tree renamed. Its working key is compared against `old_key`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenamePair {
    pub old_key: String,
    pub old_name: String,
    pub old_text_kinds: Vec<&'static str>,
}

/// Renamed working keys, each mapped to the baseline symbol it replaces.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RenameMap {
    by_new_key: BTreeMap<String, RenamePair>,
}

impl RenameMap {
    #[must_use]
    pub fn len(&self) -> usize {
        self.by_new_key.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.by_new_key.is_empty()
    }

    /// The pair whose working key is `new_key`, when that symbol was renamed.
    #[must_use]
    pub fn pair_for(&self, new_key: &str) -> Option<&RenamePair> {
        self.by_new_key.get(new_key)
    }

    /// The baseline key a working key is compared against: its old key when renamed, otherwise itself.
    #[must_use]
    pub fn was_key<'a>(&'a self, key: &'a str) -> &'a str {
        self.pair_for(key).map_or(key, |p| p.old_key.as_str())
    }

    /// True when an acknowledgement keyed by `ack_key` names a symbol that was renamed away, so it carries
    /// over to the new name.
    #[must_use]
    pub fn carries(&self, ack_key: &str) -> bool {
        self.by_new_key
            .values()
            .any(|p| p.old_name == ack_key || p.old_key == ack_key)
    }
}

/// Fewer normalized tokens than this and the body is empty: an empty body would match every other.
const MIN_RENAME_BODY_TOKENS: usize = 1;

fn fingerprint_symbol(sym: &Symbol, file_bytes: &[u8]) -> Option<BodyFingerprint> {
    let body = symbol_body_tokens(sym, file_bytes, MIN_RENAME_BODY_TOKENS)?;
    let decl = String::from_utf8_lossy(sym.decl_bytes(file_bytes));
    let text_kinds = body_text_kinds(&mask_source(&decl, sym.lang));
    Some(BodyFingerprint {
        kind: sym.kind,
        body,
        text_kinds,
    })
}

/// Fills `snap.body_by_sym` for the callables defined in `changed_files`, reading each file once.
pub fn attach_body_fingerprints(
    snap: &mut QualitySnapshot,
    ingest: &IngestResult,
    interner: &Interner,
    changed_files: &[String],
    sources: &dyn SourceReader,
) {
    let mut files: HashMap<&str, Cow<'_, [u8]>> = HashMap::new();
    for sym in &ingest.symbols {
        let path = ingest.sym_file_path(sym, interner);
        if !matches!(sym.kind, SymKind::Function | SymKind::Method)
            || !is_changed_callable(sym, path, changed_files)
        {
            continue;
        }
        let bytes = match files.entry(path) {
            Entry::Occupied(entry) => entry.into_mut(),
            Entry::Vacant(entry) => match sources.read(path) {
                Ok(bytes) => entry.insert(bytes),
                Err(_) => continue,
            },
        };
        if let Some(fp) = fingerprint_symbol(sym, bytes) {
            let scope = sym.scope.map(|sc| interner.resolve(&sc));
            let key = symbol_canonical_key(interner.resolve(&sym.name), path, scope);
            snap.body_by_sym.insert(key, fp);
        }
    }
}

/// Similarity of a vanished body to an appeared one under the clone detector's rule. An identical normalized
/// body scores 1.0 (a pure rename). Otherwise the sketch gate and the Type-3 floor decide.
fn rename_score(old: &BodyFingerprint, new: &BodyFingerprint) -> Option<f64> {
    if old.body.norm_hash == new.body.norm_hash {
        return Some(1.0);
    }
    if !sketch_stage_passes(&old.body, &new.body) {
        return None;
    }
    type3_similarity(&old.body, &new.body)
}

/// The index of the single highest-scoring option. `None` when there are no options or the top score ties.
fn unique_best(options: &[(usize, f64)]) -> Option<usize> {
    let best = options.iter().map(|&(_, s)| s).reduce(f64::max)?;
    let mut top = options.iter().filter(|&&(_, s)| s == best);
    let &(index, _) = top.next()?;
    top.next().is_none().then_some(index)
}

fn last_segment(key: &str) -> &str {
    key.rsplit("::").next().unwrap_or(key)
}

struct Candidate<'a> {
    key: &'a str,
    fp: &'a BodyFingerprint,
    arity: u16,
}

fn sorted_entries(map: &HashMap<String, BodyFingerprint>) -> Vec<(&str, &BodyFingerprint)> {
    let mut entries: Vec<_> = map.iter().map(|(k, v)| (k.as_str(), v)).collect();
    entries.sort_unstable_by(|a, b| a.0.cmp(b.0));
    entries
}

/// Pairs baseline callables that vanished with working callables that appeared, when they are one entity renamed.
#[must_use]
pub fn pair_renames(working: &QualitySnapshot, baseline: &QualitySnapshot) -> RenameMap {
    let vanished: Vec<Candidate<'_>> = sorted_entries(&baseline.body_by_sym)
        .into_iter()
        .filter(|(k, _)| {
            baseline.ccx_by_sym.contains_key(*k) && !working.ccx_by_sym.contains_key(*k)
        })
        .filter_map(|(key, fp)| {
            Some(Candidate {
                key,
                fp,
                arity: *baseline.params_by_sym.get(key)?,
            })
        })
        .collect();
    let appeared: Vec<Candidate<'_>> = sorted_entries(&working.body_by_sym)
        .into_iter()
        .filter(|(k, _)| {
            !baseline.ccx_by_sym.contains_key(*k) && working.ccx_by_sym.contains_key(*k)
        })
        .filter_map(|(key, fp)| {
            Some(Candidate {
                key,
                fp,
                arity: *working.params_by_sym.get(key)?,
            })
        })
        .collect();

    let mut from_vanished: Vec<Vec<(usize, f64)>> = vec![Vec::new(); vanished.len()];
    let mut from_appeared: Vec<Vec<(usize, f64)>> = vec![Vec::new(); appeared.len()];
    for (i, old) in vanished.iter().enumerate() {
        for (j, new) in appeared.iter().enumerate() {
            if old.fp.kind != new.fp.kind || old.arity != new.arity {
                continue;
            }
            if let Some(score) = rename_score(old.fp, new.fp) {
                from_vanished[i].push((j, score));
                from_appeared[j].push((i, score));
            }
        }
    }

    let mut by_new_key = BTreeMap::new();
    for (i, old) in vanished.iter().enumerate() {
        let Some(j) = unique_best(&from_vanished[i]) else {
            continue;
        };
        if unique_best(&from_appeared[j]) != Some(i) {
            continue;
        }
        by_new_key.insert(
            appeared[j].key.to_string(),
            RenamePair {
                old_key: old.key.to_string(),
                old_name: last_segment(old.key).to_string(),
                old_text_kinds: old.fp.text_kinds.clone(),
            },
        );
    }
    RenameMap { by_new_key }
}
