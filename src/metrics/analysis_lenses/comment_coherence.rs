use crate::domain::{IngestResult, Interner, SymKind};
use crate::serialize::{PageRequest, PageWindow, XmlEscapeExt};
use std::io::{self, Write};

#[derive(Debug, Clone, PartialEq)]
pub struct CommentCoherenceRow {
    pub path: String,
    pub line: u32,
    pub name: String,
    pub words: usize,
    pub restate: usize,
    pub c_terms: usize,
    pub i_terms: usize,
    pub shared: usize,
    pub c_coeff: f64,
    pub cic: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CommentCoherenceReport {
    pub documented: usize,
    pub no_comment: usize,
    pub rows: Vec<CommentCoherenceRow>,
    pub page: PageRequest,
}

fn split_identifier_subtokens(ident: &str) -> std::collections::BTreeSet<String> {
    let mut tokens: std::collections::BTreeSet<String> =
        super::super::eval::split_camel_or_snake(ident)
            .into_iter()
            .collect();
    if tokens.is_empty() && !ident.is_empty() {
        tokens.insert(ident.to_ascii_lowercase());
    }
    tokens
}

fn is_comment_line(trimmed: &str) -> bool {
    trimmed.starts_with("//")
        || trimmed.starts_with("/*")
        || trimmed.starts_with('*')
        || trimmed.starts_with('#')
}

fn is_doc_boundary_line(trimmed: &str) -> bool {
    trimmed.is_empty() || trimmed.starts_with("#[") || trimmed.starts_with('@')
}

fn extract_preceding_doc_words(bytes: &[u8], sig_start: u32) -> Vec<String> {
    let end = (sig_start as usize).min(bytes.len());
    let Ok(prefix) = std::str::from_utf8(&bytes[..end]) else {
        return Vec::new();
    };
    let mut doc_lines = Vec::new();
    for line in prefix.lines().rev() {
        let trimmed = line.trim();
        if doc_lines.is_empty() && is_doc_boundary_line(trimmed) {
            continue;
        }
        if !is_comment_line(trimmed) || trimmed.starts_with("//!") {
            break;
        }
        doc_lines.push(trimmed);
    }
    doc_lines.reverse();
    doc_lines
        .into_iter()
        .flat_map(|l| l.split(|c: char| !c.is_ascii_alphanumeric()))
        .filter(|w| w.len() >= 2)
        .map(str::to_ascii_lowercase)
        .collect()
}

fn measure_comment_coherence_row(
    sym: &crate::domain::Symbol,
    file_path: &str,
    name: &str,
    file_bytes: Option<&[u8]>,
) -> Option<CommentCoherenceRow> {
    let ident_terms = split_identifier_subtokens(name);
    let comment_words = file_bytes
        .map(|bytes| extract_preceding_doc_words(bytes, sym.sig_start_byte))
        .unwrap_or_default();
    let words = comment_words.len();
    if words < 2 {
        return None;
    }
    let restate = comment_words
        .iter()
        .filter(|w| {
            ident_terms
                .iter()
                .any(|it| w.contains(it) || it.contains(*w))
        })
        .count();
    let c_set: std::collections::BTreeSet<String> = comment_words.into_iter().collect();
    let c_terms = c_set.len();
    let i_terms = ident_terms.len();
    let shared = c_set
        .iter()
        .filter(|w| {
            ident_terms
                .iter()
                .any(|it| w.contains(it) || it.contains(*w))
        })
        .count();
    let union = (c_terms + i_terms).saturating_sub(shared).max(1);
    let c_coeff = (restate as f64) / (words.max(1) as f64);
    let cic = (shared as f64) / (union as f64);
    Some(CommentCoherenceRow {
        path: file_path.to_string(),
        line: sym.line,
        name: name.to_string(),
        words,
        restate,
        c_terms,
        i_terms,
        shared,
        c_coeff,
        cic,
    })
}

pub fn compute_comment_coherence(
    ingest: &IngestResult,
    interner: &Interner,
    page: PageRequest,
) -> CommentCoherenceReport {
    compute_comment_coherence_in_root(".", ingest, interner, page)
}

pub fn compute_comment_coherence_in_root(
    root: &str,
    ingest: &IngestResult,
    interner: &Interner,
    page: PageRequest,
) -> CommentCoherenceReport {
    let sources = crate::query::FileSources {
        root: std::path::Path::new(root),
    };
    compute_comment_coherence_with_sources(&sources, ingest, interner, page)
}

pub fn compute_comment_coherence_with_sources<S: crate::query::SourceReader + ?Sized>(
    sources: &S,
    ingest: &IngestResult,
    interner: &Interner,
    page: PageRequest,
) -> CommentCoherenceReport {
    let mut rows = Vec::new();
    let mut documented = 0usize;
    let mut no_comment = 0usize;
    let mut file_cache: std::collections::HashMap<u32, Option<Vec<u8>>> =
        std::collections::HashMap::new();

    use crate::domain::CorpusView;
    let view = (ingest, interner);
    for sym in &ingest.symbols {
        if (sym.kind != SymKind::Function && sym.kind != SymKind::Method)
            || sym.is_bodyless_declaration()
            || view.is_test_symbol(sym)
        {
            continue;
        }
        let file_path = view.sym_path(sym);
        let name = view.sym_name(sym);
        let file_bytes = file_cache
            .entry(sym.file_id)
            .or_insert_with(|| sources.read(file_path).ok().map(|c| c.into_owned()))
            .as_deref();
        if let Some(row) = measure_comment_coherence_row(sym, file_path, name, file_bytes) {
            documented += 1;
            rows.push(row);
        } else {
            no_comment += 1;
        }
    }

    rows.sort_by(|a, b| {
        b.c_coeff
            .partial_cmp(&a.c_coeff)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.path.cmp(&b.path))
    });

    CommentCoherenceReport {
        documented,
        no_comment,
        rows,
        page,
    }
}

impl CommentCoherenceReport {
    fn window(&self) -> PageWindow {
        crate::serialize::window_rows(self.page, &self.rows, |r, w| {
            write_comment_coherence_row(r, w)
        })
    }
}

fn write_comment_coherence_row<W: Write + ?Sized>(
    r: &CommentCoherenceRow,
    w: &mut W,
) -> io::Result<()> {
    writeln!(
        w,
        "  <fn p=\"{}:{}\" n=\"{}\" c_coeff=\"{:.3}\" words=\"{}\" restate=\"{}\" cic=\"{:.3}\" c_terms=\"{}\" i_terms=\"{}\" shared=\"{}\"/>",
        r.path.xml_escaped(),
        r.line,
        r.name.xml_escaped(),
        r.c_coeff,
        r.words,
        r.restate,
        r.cic,
        r.c_terms,
        r.i_terms,
        r.shared
    )
}

impl crate::serialize::XmlReport for CommentCoherenceReport {
    crate::xml_report_meta!(
        "comment_coherence",
        "ripwire-rs.comment-coherence/v1",
        root: true,
        comment: "ripwire-rs comment-coherence schema=ripwire-rs.comment-coherence/v1: two comment/name content measures per documented function."
    );
    fn page_window(&self) -> Option<crate::serialize::PageWindow> {
        Some(self.window())
    }
    fn write_extra_attrs<W: Write>(&self, w: &mut W) -> io::Result<()> {
        write!(
            w,
            " documented=\"{}\" no_comment=\"{}\"",
            self.documented, self.no_comment
        )
    }
    fn write_body<W: Write>(&self, w: &mut W) -> io::Result<()> {
        crate::serialize::write_window_rows(
            &self.rows,
            &self.window(),
            w,
            write_comment_coherence_row,
        )
    }
}
