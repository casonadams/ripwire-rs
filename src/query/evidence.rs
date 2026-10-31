//! What in a symbol mentions each query term: the functions it calls, its own text, and its doc comment.
//!
//! Bit `i` of a mask is query term `i`. A symbol's name, scope and path are scored separately; this is the
//! evidence found inside and around it.
use super::retrieval::stem_plural_only;
use crate::domain::{IngestResult, Interner, SymKind, Symbol};
use memchr::memmem::Finder;

/// What a term in a doc comment is worth: the author wrote it to say what the code is for.
const DOC_TERM_WEIGHT: f64 = 8.0;
/// What a call into a function named for the term is worth: the symbol coordinates that concept.
const CALL_TERM_WEIGHT: f64 = 6.0;
/// What a term in a body of `TEXT_REFERENCE_LOC` lines or fewer is worth; a longer body contains terms by
/// chance, so its weight falls with the square root of its length.
const TEXT_TERM_WEIGHT: f64 = 6.0;
const TEXT_REFERENCE_LOC: f64 = 30.0;
/// Query terms searched for in source text, and the width of a mask.
const MAX_TERMS: usize = 16;
/// Most line comments read above a definition as its documentation.
const MAX_DOC_LINES: usize = 12;

/// Per symbol, the query terms each kind of evidence contains, indexed by symbol id.
pub(super) struct TermEvidence {
    calls: Vec<u32>,
    text: Vec<u32>,
    doc: Vec<u32>,
    loc: Vec<u32>,
}

/// One symbol's slice of [`TermEvidence`].
#[derive(Clone, Copy, Default)]
pub(super) struct SymbolEvidence {
    calls: u32,
    text: u32,
    doc: u32,
    loc: u32,
}

impl SymbolEvidence {
    /// What query term `i` is worth here, by the strongest evidence that contains it.
    pub(super) fn term_weight(self, i: usize) -> f64 {
        let bit = 1u32 << i.min(31);
        if self.doc & bit != 0 {
            DOC_TERM_WEIGHT
        } else if self.calls & bit != 0 {
            CALL_TERM_WEIGHT
        } else if self.text & bit != 0 {
            TEXT_TERM_WEIGHT
                * (TEXT_REFERENCE_LOC / f64::from(self.loc).max(TEXT_REFERENCE_LOC)).sqrt()
        } else {
            0.0
        }
    }

    /// Whether any evidence mentions query term `i`.
    pub(super) fn mentions(self, i: usize) -> bool {
        i < 32 && (self.calls | self.text | self.doc) & (1u32 << i) != 0
    }
}

impl TermEvidence {
    /// Evidence from call targets alone, which every query has; [`Self::add_text`] adds the rest.
    pub(super) fn from_calls(
        ingest: &IngestResult,
        interner: &Interner,
        raw_terms: &[&str],
    ) -> Self {
        let n = ingest.symbols.len();
        Self {
            calls: call_masks(ingest, interner, raw_terms),
            text: vec![0; n],
            doc: vec![0; n],
            loc: ingest.symbols.iter().map(|sym| sym.loc).collect(),
        }
    }

    pub(super) fn of(&self, sym: &Symbol) -> SymbolEvidence {
        let at = sym.id as usize;
        let get = |masks: &[u32]| masks.get(at).copied().unwrap_or(0);
        SymbolEvidence {
            calls: get(&self.calls),
            text: get(&self.text),
            doc: get(&self.doc),
            loc: self.loc.get(at).copied().unwrap_or(0),
        }
    }

    /// Marks symbols whose own source text, or the doc comment above it, contains a query term
    /// (case-insensitive). Each file is read and lowercased once; the text is searched as slices of that
    /// copy. A constant or a bodyless declaration carries its doc comment as evidence too.
    pub(super) fn add_text(
        &mut self,
        ingest: &IngestResult,
        interner: &Interner,
        sources: &dyn crate::query::SourceReader,
        raw_terms: &[&str],
    ) {
        let finders = term_finders(raw_terms);
        let mut code: Vec<&Symbol> = ingest
            .symbols
            .iter()
            .filter(|sym| sym.is_code_symbol())
            .collect();
        code.sort_by_key(|sym| sym.file_id);
        for syms in code.chunk_by(|a, b| a.file_id == b.file_id) {
            let path = ingest.sym_file_path(syms[0], interner);
            let lower = sources
                .read(path)
                .map(|bytes| bytes.to_ascii_lowercase())
                .unwrap_or_default();
            for sym in syms {
                let (body, doc) = own_text(&lower, sym);
                self.text[sym.id as usize] |= term_bits(&finders, body);
                self.doc[sym.id as usize] |= term_bits(&finders, doc);
            }
        }
    }
}

/// Per-term needles (lowercase term and its stem) searched in source text.
fn term_finders(raw_terms: &[&str]) -> Vec<Vec<Finder<'static>>> {
    raw_terms
        .iter()
        .take(MAX_TERMS)
        .map(|&t| {
            let low = t.to_ascii_lowercase();
            let stem = stem_plural_only(&low);
            let mut out = vec![Finder::new(low.as_bytes()).into_owned()];
            if stem != low {
                out.push(Finder::new(stem.as_bytes()).into_owned());
            }
            out
        })
        .collect()
}

/// Whether a lowercase text contains the needle; short needles must stand as whole tokens.
fn contains_needle(text: &[u8], finder: &Finder<'_>) -> bool {
    let needle_len = finder.needle().len();
    if needle_len >= 4 {
        return finder.find(text).is_some();
    }
    finder.find_iter(text).any(|at| {
        let before = at == 0 || !text[at - 1].is_ascii_alphanumeric();
        let after = text
            .get(at + needle_len)
            .is_none_or(|b| !b.is_ascii_alphanumeric());
        before && after
    })
}

/// Bit `i` is set when any needle of query term `i` occurs in `text`.
fn term_bits(finders: &[Vec<Finder<'static>>], text: &[u8]) -> u32 {
    finders
        .iter()
        .enumerate()
        .filter(|(_, needles)| needles.iter().any(|f| contains_needle(text, f)))
        .fold(0, |bits, (i, _)| bits | 1u32 << i)
}

/// A `//` comment, or a `#` comment (`#` then whitespace, so `#include` and `#[attr]` are not comments).
fn is_line_comment(line: &[u8]) -> bool {
    let text = line.trim_ascii_start();
    text.starts_with(b"//")
        || (text.starts_with(b"#") && text.get(1).is_none_or(u8::is_ascii_whitespace))
}

/// The text documenting `sym`: its attached doc comment, else the run of line comments directly above it,
/// which many codebases write instead of doc blocks. Indexing keeps an attached doc to the languages' own
/// rules (an edit splices it with the definition); retrieval only reads, so it takes both.
fn leading_doc<'a>(lower: &'a [u8], sym: &Symbol) -> &'a [u8] {
    let sig = (sym.sig_start_byte as usize).min(lower.len());
    if sym.doc_len > 0 {
        return lower
            .get(sig.saturating_sub(sym.doc_len as usize)..sig)
            .unwrap_or_default();
    }
    let line_start = |before: usize| {
        lower[..before]
            .iter()
            .rposition(|&b| b == b'\n')
            .map_or(0, |i| i + 1)
    };
    let end = line_start(sig);
    let mut start = end;
    for _ in 0..MAX_DOC_LINES {
        if start == 0 {
            break;
        }
        let prev = line_start(start - 1);
        if !is_line_comment(&lower[prev..start - 1]) {
            break;
        }
        start = prev;
    }
    &lower[start..end]
}

/// A symbol's own text (a constant or field is all declaration) and its doc, as slices of its lowercased file.
fn own_text<'a>(lower: &'a [u8], sym: &Symbol) -> (&'a [u8], &'a [u8]) {
    let has_text = sym.has_body() || matches!(sym.kind, SymKind::Var | SymKind::Field);
    let body = if has_text { sym.decl_bytes(lower) } else { &[] };
    (body, leading_doc(lower, sym))
}

/// Per symbol, the query terms named by something it calls or the qualifier it calls through.
fn call_masks(ingest: &IngestResult, interner: &Interner, raw_terms: &[&str]) -> Vec<u32> {
    let mut masks = vec![0u32; ingest.symbols.len()];
    let stems: Vec<(String, String)> = raw_terms
        .iter()
        .take(MAX_TERMS)
        .map(|&t| {
            let low = t.to_ascii_lowercase();
            let stem = stem_plural_only(&low);
            (low, stem)
        })
        .collect();
    for r in &ingest.references {
        let Some(mask) = masks.get_mut(r.from_symbol as usize) else {
            continue;
        };
        let callee = interner.resolve(&r.callee_name).to_ascii_lowercase();
        let qual = r
            .qualifier
            .map(|q| interner.resolve(&q).to_ascii_lowercase());
        for (i, (low, stem)) in stems.iter().enumerate() {
            let named = |text: &str| text.contains(low) || text.contains(stem);
            if named(&callee) || qual.as_deref().is_some_and(named) {
                *mask |= 1u32 << i;
            }
        }
    }
    masks
}
