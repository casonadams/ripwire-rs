use crate::clones::token_stream::{
    scan_ident_slice, scan_number_slice, scan_operator_slice, scan_string_slice, skip_whitespace,
};
use crate::domain::Lang;
use std::collections::HashMap;
pub const POSNETT_INTERCEPT: f64 = 8.87;
pub const POSNETT_VOLUME: f64 = -0.033;
pub const POSNETT_LINES: f64 = 0.40;
pub const POSNETT_ENTROPY: f64 = -1.5;
pub const POSNETT_Z_CLAMP: f64 = 40.0;

/// Readability metrics computed for a symbol or function definition.
#[derive(Debug, Clone, PartialEq)]
pub struct ReadabilityMetrics {
    pub token_count: u32,
    pub operator_count: u32,
    pub operand_count: u32,
    pub vocabulary_count: u32,
    pub volume: f64,
    pub entropy: f64,
    pub lines: u32,
    pub posnett: f64,
}

/// Compute the Posnett/Hindle/Devanbu (MSR 2011) readability score.
#[must_use]
pub fn posnett_score(volume: f64, lines: f64, entropy: f64) -> f64 {
    let z = POSNETT_INTERCEPT
        + POSNETT_VOLUME * volume
        + POSNETT_LINES * lines
        + POSNETT_ENTROPY * entropy;
    let clamped = z.clamp(-POSNETT_Z_CLAMP, POSNETT_Z_CLAMP);
    1.0 / (1.0 + (-clamped).exp())
}

#[derive(Debug, PartialEq, Eq)]
enum TokenKind {
    Operator,
    Operand,
}

fn tokenize_code(code: &str, lang: Lang) -> Vec<(&[u8], TokenKind)> {
    let bytes = code.as_bytes();
    let mut tokens = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            skip_whitespace(bytes, &mut i);
        } else if b == b'"' || b == b'\'' {
            let lit = scan_string_slice(bytes, &mut i);
            tokens.push((lit, TokenKind::Operand));
        } else if b.is_ascii_alphabetic() || b == b'_' {
            let word = scan_ident_slice(bytes, &mut i);
            let kind = if lang.semantics().is_keyword(word) {
                TokenKind::Operator
            } else {
                TokenKind::Operand
            };
            tokens.push((word, kind));
        } else if b.is_ascii_digit() {
            let num = scan_number_slice(bytes, &mut i);
            tokens.push((num, TokenKind::Operand));
        } else {
            let sym = scan_operator_slice(bytes, &mut i);
            tokens.push((sym, TokenKind::Operator));
        }
    }
    tokens
}

fn calculate_entropy(frequencies: &[u32], total: f64) -> f64 {
    if total <= 0.0 {
        return 0.0;
    }
    let mut entropy = 0.0;
    for &freq in frequencies {
        let p = (freq as f64) / total;
        if p > 0.0 {
            entropy -= p * p.log2();
        }
    }
    entropy
}

/// Compute Halstead Volume, Token Entropy, and Posnett Readability score.
#[must_use]
pub fn compute_readability(code: &str, lang: Lang) -> ReadabilityMetrics {
    let tokens = tokenize_code(code, lang);
    let token_count = tokens.len() as u32;
    let lines = (code.lines().count() as u32).max(1);

    if tokens.is_empty() {
        return ReadabilityMetrics {
            token_count: 0,
            operator_count: 0,
            operand_count: 0,
            vocabulary_count: 0,
            volume: 0.0,
            entropy: 0.0,
            lines,
            posnett: posnett_score(0.0, lines as f64, 0.0),
        };
    }

    let mut vocab_map: HashMap<&[u8], u32> = HashMap::new();
    let mut operator_count = 0u32;
    let mut operand_count = 0u32;

    for (text, kind) in &tokens {
        *vocab_map.entry(*text).or_insert(0) += 1;
        match kind {
            TokenKind::Operator => operator_count += 1,
            TokenKind::Operand => operand_count += 1,
        }
    }

    let vocabulary_count = vocab_map.len() as u32;
    let volume = if vocabulary_count > 1 {
        (token_count as f64) * (vocabulary_count as f64).log2()
    } else {
        0.0
    };

    let frequencies: Vec<u32> = vocab_map.into_values().collect();
    let entropy = calculate_entropy(&frequencies, token_count as f64);
    let posnett = posnett_score(volume, lines as f64, entropy);

    ReadabilityMetrics {
        token_count,
        operator_count,
        operand_count,
        vocabulary_count,
        volume,
        entropy,
        lines,
        posnett,
    }
}

pub struct ReadabilityRow<'a> {
    pub sym: &'a crate::domain::Symbol,
    pub path: &'a str,
    pub name: &'a str,
    pub metrics: ReadabilityMetrics,
}

pub struct ReadabilityReport<'a> {
    pub functions: Vec<ReadabilityRow<'a>>,
    pub page: crate::serialize::PageRequest,
}

impl ReadabilityReport<'_> {
    fn window(&self) -> crate::serialize::PageWindow {
        crate::serialize::window_rows(self.page, &self.functions, |f, w| {
            write_readability_row(f, w)
        })
    }
}

fn write_readability_row<W: std::io::Write + ?Sized>(
    row: &ReadabilityRow<'_>,
    out: &mut W,
) -> std::io::Result<()> {
    let lines = row.sym.loc.max(1);
    let m = &row.metrics;
    let posnett = posnett_score(m.volume, lines as f64, m.entropy);
    writeln!(
        out,
        "  <fn p=\"{}:{}\" n=\"{}\" lines=\"{lines}\" toks=\"{}\" ops=\"{}\" vocab=\"{}\" vol=\"{:.0}\" ent=\"{:.2}\" posnett=\"{posnett:.2}\"/>",
        row.path,
        row.sym.line,
        row.name,
        m.token_count,
        m.operator_count,
        m.vocabulary_count,
        m.volume,
        m.entropy
    )
}

impl crate::serialize::XmlReport for ReadabilityReport<'_> {
    crate::xml_report_meta!(
        "readability",
        "ripwire-rs.readability/v1",
        root: true,
        comment: "ripwire-rs readability schema=ripwire-rs.readability/v1: Posnett/Hindle/Devanbu lens, largest Halstead volume first: <fn p= n= lines= toks= ops= vocab= vol= ent= posnett=>."
    );
    fn page_window(&self) -> Option<crate::serialize::PageWindow> {
        Some(self.window())
    }
    fn write_extra_attrs<W: std::io::Write>(&self, out: &mut W) -> std::io::Result<()> {
        write!(out, " functions=\"{}\"", self.functions.len())
    }
    fn write_body<W: std::io::Write>(&self, out: &mut W) -> std::io::Result<()> {
        crate::serialize::write_window_rows(
            &self.functions,
            &self.window(),
            out,
            write_readability_row,
        )
    }
}

/// Output biggest functions first based on volume and lines.
pub fn execute_biggest_first_query<W: std::io::Write>(
    root: &str,
    ingest: &crate::domain::IngestResult,
    interner: &crate::domain::Interner,
    page: crate::serialize::PageRequest,
    out: &mut W,
) -> std::io::Result<()> {
    use crate::domain::CorpusView;
    use crate::query::SourceReader;
    use crate::serialize::XmlReport;
    let view = (ingest, interner);
    let sources = crate::query::FileSources {
        root: std::path::Path::new(root),
    };
    let mut files: HashMap<&str, Vec<u8>> = HashMap::new();
    let mut functions: Vec<ReadabilityRow<'_>> = ingest
        .symbols
        .iter()
        .filter(|sym| {
            !sym.is_bodyless_declaration()
                && matches!(
                    sym.kind,
                    crate::domain::SymKind::Function | crate::domain::SymKind::Method
                )
        })
        .map(|sym| {
            let path = view.sym_path(sym);
            let source = files.entry(path).or_insert_with(|| {
                sources
                    .read(path)
                    .map(std::borrow::Cow::into_owned)
                    .unwrap_or_default()
            });
            let body = source
                .get(sym.sig_start_byte as usize..sym.end_byte as usize)
                .unwrap_or_default();
            ReadabilityRow {
                sym,
                path,
                name: view.sym_name(sym),
                metrics: compute_readability(&String::from_utf8_lossy(body), sym.lang),
            }
        })
        .collect();
    functions.sort_by(|a, b| {
        b.sym
            .loc
            .cmp(&a.sym.loc)
            .then_with(|| b.sym.ccx.cmp(&a.sym.ccx))
    });
    ReadabilityReport { functions, page }.write_xml_report(root, out)
}
