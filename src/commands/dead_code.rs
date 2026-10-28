//! `--dead-code`: internal functions with zero incoming callers; honors `--limit` / `--offset`.
use super::SharedOption;
use super::plugin::report_command;
use crate::commands::context::ExecutionContext;
use crate::domain::{IngestResult, Interner};
use crate::graph::Graph;
use crate::serialize::XmlWriter;
use std::io::{self, Write};
use std::path::Path;

report_command! {
    name: "dead-code",
    args: DeadCodeArgs,
    static: DEAD_CODE,
    options: [SharedOption::Paged],
    run: report(render),
    /// Internal functions with zero incoming callers across the repository
    #[arg(long = "dead-code")]
    dead_code: bool,
}

struct CachedFile<'a> {
    source: std::borrow::Cow<'a, [u8]>,
    tree: Option<tree_sitter::Tree>,
}

pub struct DeadCodeContext<'a> {
    pub root: &'a str,
    pub sources: &'a dyn crate::query::SourceReader,
    pub ingest: &'a IngestResult,
    pub graph: &'a Graph,
    pub interner: &'a Interner,
    pub page: &'a dyn crate::commands::context::Paginated,
}

pub fn execute_dead_code_query_paged<W: Write>(
    ctx: &DeadCodeContext<'_>,
    out: &mut W,
) -> io::Result<()> {
    use crate::domain::CorpusView;
    let view = (ctx.ingest, ctx.interner);
    let mut file_cache: std::collections::HashMap<u32, CachedFile<'_>> =
        std::collections::HashMap::new();
    let mut candidates = Vec::new();
    for sym in &ctx.ingest.symbols {
        let p = view.sym_path(sym);
        let semantics = sym.lang.semantics();
        if sym.kind != crate::domain::SymKind::Function
            || !semantics.supports_internal_linkage()
            || sym.sig_end_byte >= sym.end_byte
            || ctx.graph.in_degree(sym.id) != 0
            || semantics.is_header_file(p)
        {
            continue;
        }
        let cached = file_cache.entry(sym.file_id).or_insert_with(|| {
            let source = ctx
                .sources
                .read(p)
                .unwrap_or(std::borrow::Cow::Borrowed(&[]));
            let tree = crate::ingest::identifiers::parse_file_tree(sym.lang, p, source.as_ref());
            CachedFile { source, tree }
        });
        let is_internal = cached.tree.as_ref().is_some_and(|tree| {
            sym.lang.semantics().is_internal_function(
                tree.root_node(),
                sym.sig_start_byte as usize,
                cached.source.as_ref(),
            )
        });
        if is_internal {
            candidates.push(sym);
        }
    }
    let mut uses_by_lang: std::collections::HashMap<crate::domain::Lang, std::collections::HashMap<&str, u32>> =
        std::collections::HashMap::new();
    for sym in &candidates {
        uses_by_lang
            .entry(sym.lang)
            .or_default()
            .insert(view.sym_name(sym), 0);
    }
    for (lang, uses) in &mut uses_by_lang {
        crate::ingest::identifiers::count_identifier_uses(ctx.ingest, ctx.interner, ctx.sources, *lang, uses);
    }
    let mut dead = Vec::new();
    for sym in candidates {
        let name = view.sym_name(sym);
        let referenced = uses_by_lang
            .get(&sym.lang)
            .and_then(|uses| uses.get(name))
            .is_some_and(|&count| count > 0);
        if !referenced {
            dead.push((name, sym.kind.tag(), view.sym_path(sym), sym.line));
        }
    }
    dead.sort_by(|a, b| {
        a.2.cmp(b.2)
            .then_with(|| a.3.cmp(&b.3))
            .then_with(|| a.0.cmp(b.0))
    });
    crate::serialize::query_xml::serialize_dead_code_paged_xml(
        ctx.root,
        &dead,
        ctx.graph.gauges(),
        ctx.page.page_request(),
        out,
    )
}

pub fn render<W: Write>(
    ctx: &ExecutionContext<'_>,
    _args: &DeadCodeArgs,
    out: &mut XmlWriter<'_, W>,
) -> io::Result<()> {
    execute_dead_code_query_paged(
        &DeadCodeContext {
            root: ctx.root,
            sources: &crate::query::FileSources {
                root: Path::new(ctx.root),
            },
            ingest: ctx.ingest,
            graph: ctx.graph,
            interner: ctx.interner,
            page: ctx.cli,
        },
        out,
    )
}
