use super::lint::{LintFinding, LintRuleDesc};
use super::lint_naming::NameCorpus;
use crate::domain::{IngestResult, Interner, Lang, SymKind, Symbol};
use crate::ingest::registry::parse_with_grammar;
use crate::languages::LanguageSemantics;

#[derive(Clone, Copy)]
pub struct FileSym<'a> {
    pub sym: &'a Symbol,
    pub name: &'a str,
    pub scope: Option<&'a str>,
}

pub struct FileLintContext<'a> {
    pub path: &'a str,
    pub lang: Lang,
    pub sem: &'static dyn LanguageSemantics,
    pub raw: &'a str,
    pub code: String,
    pub syms: Vec<FileSym<'a>>,
    pub corpus: &'a NameCorpus,
    pub tree: Option<tree_sitter::Tree>,
    line_starts: Vec<u32>,
}

impl<'a> FileLintContext<'a> {
    #[must_use]
    pub fn new(
        path: &'a str,
        lang: Lang,
        raw: &'a str,
        syms: Vec<FileSym<'a>>,
        corpus: &'a NameCorpus,
    ) -> Self {
        let newlines = raw.bytes().enumerate().filter(|&(_, b)| b == b'\n');
        let line_starts = std::iter::once(0)
            .chain(newlines.map(|(i, _)| (i + 1) as u32))
            .collect();
        let sem = lang.semantics();
        let tree = parse_source(sem, path, raw);
        let code = tree.as_ref().map_or_else(
            || super::source_mask::mask_source(raw, lang).code,
            |tree| super::source_mask::mask_source_in_tree(raw, lang, tree).code,
        );
        Self {
            path,
            lang,
            sem,
            raw,
            code,
            syms,
            corpus,
            tree,
            line_starts,
        }
    }

    #[must_use]
    pub fn line_of(&self, pos: usize) -> u32 {
        self.line_starts.partition_point(|&s| s as usize <= pos) as u32
    }

    fn callable_at(&self, pos: usize) -> Option<FileSym<'a>> {
        self.syms
            .iter()
            .copied()
            .filter(|s| {
                s.sym.is_callable()
                    && (s.sym.sig_start_byte as usize..s.sym.end_byte as usize).contains(&pos)
            })
            .min_by_key(|s| s.sym.end_byte - s.sym.sig_start_byte)
    }

    #[must_use]
    pub fn enclosing_at(&self, pos: usize) -> &'a str {
        self.callable_at(pos).map_or("", |s| s.name)
    }

    #[must_use]
    pub fn in_callable(&self, pos: usize) -> bool {
        self.callable_at(pos).is_some()
    }

    #[must_use]
    pub fn signature(&self, sym: &Symbol) -> &'a str {
        self.raw
            .get(sym.sig_start_byte as usize..sym.sig_end_byte as usize)
            .unwrap_or("")
    }

    #[must_use]
    pub fn declaration(&self, sym: &Symbol) -> &'a str {
        self.raw.get(sym.sig_start_byte as usize..).unwrap_or("")
    }

    #[must_use]
    pub fn body(&self, sym: &Symbol) -> &str {
        self.code
            .get(sym.sig_end_byte as usize..sym.end_byte as usize)
            .unwrap_or("")
    }
}

fn parse_source(
    sem: &dyn LanguageSemantics,
    path: &str,
    source: &str,
) -> Option<tree_sitter::Tree> {
    parse_with_grammar(sem.source_grammar(path)?, source.as_bytes())
}

pub struct Sink<'a> {
    desc: &'static LintRuleDesc,
    ctx: &'a FileLintContext<'a>,
    pub out: Vec<LintFinding>,
    pub capped: bool,
}

impl<'a> Sink<'a> {
    #[must_use]
    pub fn new(desc: &'static LintRuleDesc, ctx: &'a FileLintContext<'a>) -> Self {
        Self {
            desc,
            ctx,
            out: Vec::new(),
            capped: false,
        }
    }

    fn push(&mut self, pos: usize, line: u32, enclosing: &str, message: String) {
        self.out.push(LintFinding {
            rule: self.desc.name,
            path: self.ctx.path.to_string(),
            line,
            pos: pos as u32,
            enclosing: enclosing.to_string(),
            message,
        });
    }

    pub fn at(&mut self, pos: usize, message: impl Into<String>) {
        let enclosing = self.ctx.enclosing_at(pos);
        self.push(pos, self.ctx.line_of(pos), enclosing, message.into());
    }

    pub fn at_symbol(&mut self, s: FileSym<'_>, message: impl Into<String>) {
        self.push(
            s.sym.sig_start_byte as usize,
            s.sym.line,
            s.name,
            message.into(),
        );
    }
}

#[must_use]
pub fn symbols_in_file<'a>(
    ingest: &'a IngestResult,
    interner: &'a Interner,
    file_id: u32,
) -> Vec<FileSym<'a>> {
    let mut syms: Vec<FileSym<'a>> = ingest
        .symbols
        .iter()
        .filter(|s| s.file_id == file_id && s.kind != SymKind::Section)
        .map(|s| FileSym {
            sym: s,
            name: interner.resolve(&s.name),
            scope: s.scope.as_ref().and_then(|sc| interner.try_resolve(sc)),
        })
        .collect();
    syms.sort_by_key(|s| (s.sym.sig_start_byte, s.sym.line));
    syms
}

pub(crate) fn large_function(ctx: &FileLintContext<'_>, sink: &mut Sink<'_>) {
    for &s in ctx
        .syms
        .iter()
        .filter(|s| s.sym.is_callable() && s.sym.loc.saturating_sub(1) > 80)
    {
        sink.at_symbol(s, format!("{} ({} lines)", s.name, s.sym.loc - 1));
    }
}

pub(crate) fn one_line(raw: &str) -> String {
    raw.chars()
        .take(120)
        .map(|c| {
            if matches!(c, '\n' | '\r' | '\t') {
                ' '
            } else {
                c
            }
        })
        .collect()
}
