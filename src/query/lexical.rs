//! Conceptual ranking: BM25 over identifier subtokens, scored by the `bm25` crate.
//!
//! Each code symbol is one document: its name (x3), the names it calls, its doc comment (x2) and its own
//! text. A query matches code by what it does, not only by what it is named. Term frequency saturates and
//! length is normalized against the corpus average, which a hand-weighted sum of hits could not do.
use crate::domain::{IngestResult, Interner, SymKind, Symbol};
use bm25::{EmbedderBuilder, Embedding, Scorer, Tokenizer};
use rayon::prelude::*;
use std::borrow::Cow;
use std::collections::HashSet;

const NAME_COPIES: usize = 3;
const DOC_COPIES: usize = 2;
/// Most line comments read above a definition as its documentation.
const MAX_DOC_LINES: usize = 12;

/// Lowercase subtokens of identifiers and prose: `parseHTTPHeader` gives `parse`, `http`, `header`.
/// Tokens shorter than two bytes are dropped.
#[derive(Default)]
struct Subtokens;

impl Tokenizer for Subtokens {
    fn tokenize(&self, text: &str) -> Vec<String> {
        let mut out = Vec::new();
        for_each_subtoken(text, |token| out.push(token.to_ascii_lowercase()));
        out
    }
}

/// Calls `emit` with each subtoken of `text`, in order, as it appears in the text.
fn for_each_subtoken(text: &str, mut emit: impl FnMut(&str)) {
    let bytes = text.as_bytes();
    let mut start = None;
    for i in 0..=bytes.len() {
        let alnum = bytes.get(i).is_some_and(u8::is_ascii_alphanumeric);
        if let Some(from) = start
            && (!alnum || starts_word(bytes, i))
        {
            if i - from >= 2 {
                emit(&text[from..i]);
            }
            start = None;
        }
        if alnum && start.is_none() {
            start = Some(i);
        }
    }
}

/// The number of subtokens `Subtokens` yields for `text`, counted without allocating them.
fn count_subtokens(text: &str) -> usize {
    let mut count = 0;
    for_each_subtoken(text, |_| count += 1);
    count
}

/// A camelCase boundary at `i`: `aB`, `1B`, or the last capital of an acronym run (`HTTPServer` splits
/// before `S`).
fn starts_word(bytes: &[u8], i: usize) -> bool {
    if i == 0 || !bytes[i].is_ascii_uppercase() {
        return false;
    }
    let prev = bytes[i - 1];
    prev.is_ascii_lowercase()
        || prev.is_ascii_digit()
        || (prev.is_ascii_uppercase() && bytes.get(i + 1).is_some_and(u8::is_ascii_lowercase))
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
fn leading_doc<'a>(file: &'a [u8], sym: &Symbol) -> &'a [u8] {
    let sig = (sym.sig_start_byte as usize).min(file.len());
    if sym.doc_len > 0 {
        return file
            .get(sig.saturating_sub(sym.doc_len as usize)..sig)
            .unwrap_or_default();
    }
    let line_start = |before: usize| {
        file[..before]
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
        if !is_line_comment(&file[prev..start - 1]) {
            break;
        }
        start = prev;
    }
    &file[start..end]
}

/// A symbol's own text (a constant or field is all declaration) and its doc, as slices of its file.
fn own_text<'a>(file: &'a [u8], sym: &Symbol) -> (&'a [u8], &'a [u8]) {
    let has_text = sym.has_body() || matches!(sym.kind, SymKind::Var | SymKind::Field);
    let body = if has_text { sym.decl_bytes(file) } else { &[] };
    (body, leading_doc(file, sym))
}

/// BM25 scores of one query against every code symbol, and the documents they were scored on.
pub(super) struct Lexical {
    scores: Vec<f32>,
    texts: Vec<String>,
}

impl Lexical {
    /// Scores `terms` against the corpus. `sources` supplies each file's text; without it a symbol is
    /// its name and the names it calls.
    pub(super) fn build(
        ingest: &IngestResult,
        interner: &Interner,
        sources: Option<&dyn crate::query::SourceReader>,
        terms: &[&str],
    ) -> Self {
        let texts = symbol_texts(ingest, interner, sources);
        let corpus: Vec<&str> = texts
            .iter()
            .filter(|t| !t.is_empty())
            .map(String::as_str)
            .collect();
        let total_len: usize = corpus.par_iter().map(|text| count_subtokens(text)).sum();
        let avgdl = total_len as f32 / corpus.len().max(1) as f32;
        let embedder = EmbedderBuilder::<u32, Subtokens>::with_avgdl(avgdl).build();
        let ids: Vec<u32> = (0..texts.len() as u32)
            .filter(|&id| !texts[id as usize].is_empty())
            .collect();
        let embeddings: Vec<Embedding<u32>> =
            corpus.par_iter().map(|text| embedder.embed(text)).collect();
        let mut scorer: Scorer<u32, u32> = Scorer::new();
        for (id, embedding) in ids.iter().zip(embeddings) {
            scorer.upsert(id, embedding);
        }
        let mut scores = vec![0.0; texts.len()];
        for hit in scorer.matches(&embedder.embed(&terms.join(" "))) {
            scores[hit.id as usize] = hit.score;
        }
        Self { scores, texts }
    }

    pub(super) fn score(&self, sym: &Symbol) -> f64 {
        f64::from(self.scores.get(sym.id as usize).copied().unwrap_or(0.0))
    }

    /// The subtokens of `sym`'s document.
    pub(super) fn tokens(&self, sym: &Symbol) -> HashSet<String> {
        let text = self.texts.get(sym.id as usize).map_or("", String::as_str);
        Subtokens.tokenize(text).into_iter().collect()
    }
}

/// Whether `tokens` holds every subtoken of the query term.
pub(super) fn mentions(tokens: &HashSet<String>, term: &str) -> bool {
    let wanted = Subtokens.tokenize(term);
    !wanted.is_empty() && wanted.iter().all(|t| tokens.contains(t))
}

/// Per symbol id, the document BM25 scores: empty for symbols that are not code.
fn symbol_texts(
    ingest: &IngestResult,
    interner: &Interner,
    sources: Option<&dyn crate::query::SourceReader>,
) -> Vec<String> {
    let mut callees = vec![String::new(); ingest.symbols.len()];
    for r in &ingest.references {
        if let Some(names) = callees.get_mut(r.from_symbol as usize) {
            names.push_str(interner.resolve(&r.callee_name));
            names.push(' ');
        }
    }
    let mut texts = vec![String::new(); ingest.symbols.len()];
    let mut code: Vec<&Symbol> = ingest
        .symbols
        .iter()
        .filter(|sym| sym.is_code_symbol())
        .collect();
    code.sort_by_key(|sym| sym.file_id);
    let groups: Vec<&[&Symbol]> = code.chunk_by(|a, b| a.file_id == b.file_id).collect();
    let documents: Vec<Vec<(usize, String)>> = groups
        .par_iter()
        .map(|syms| {
            let path = ingest.sym_file_path(syms[0], interner);
            let file: Cow<'_, [u8]> = match sources {
                Some(sources) => sources.read(path).unwrap_or_default(),
                None => Cow::Borrowed(&[]),
            };
            syms.iter()
                .map(|sym| {
                    let callee = &callees[sym.id as usize];
                    (
                        sym.id as usize,
                        symbol_document(&file, sym, interner, callee),
                    )
                })
                .collect()
        })
        .collect();
    for (id, text) in documents.into_iter().flatten() {
        texts[id] = text;
    }
    texts
}

/// One symbol's BM25 document: its name, the names it calls, its doc comment and its own text.
fn symbol_document(file: &[u8], sym: &Symbol, interner: &Interner, callees: &str) -> String {
    let (body, doc) = own_text(file, sym);
    let doc = String::from_utf8_lossy(doc);
    let name = interner.resolve(&sym.name);
    let mut text = String::new();
    for _ in 0..NAME_COPIES {
        text.push_str(name);
        text.push(' ');
    }
    text.push_str(callees);
    for _ in 0..DOC_COPIES {
        text.push_str(&doc);
        text.push(' ');
    }
    text.push_str(&String::from_utf8_lossy(body));
    text
}
