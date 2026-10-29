use super::extract::{FileParseResult, RawDef, parse_file_tree};
use super::ownership::{assign_module_scope_owner, build_file_references};
use super::registry::LanguageRegistry;
use crate::crawler::CrawledFile;
use crate::domain::{
    CallableBinding, FileInfo, ForeignBinding, IngestResult, Interner, Lang, NodeId, Reference,
    Symbol,
};
use rayon::prelude::*;
use std::fs;
use tree_sitter::{Language, Query};

/// Parse a single file content with a compiled grammar and query.
pub fn parse_file_content(
    source_bytes: &[u8],
    language: &Language,
    query: &Query,
    lang: Lang,
    rel_path: &str,
) -> FileParseResult {
    if let Some(overridden) =
        lang.semantics()
            .parse_file_override(source_bytes, rel_path, language, query)
    {
        return overridden;
    }
    parse_file_tree(source_bytes, language, query, lang)
}

/// Definitions the index extracts from `source_bytes` as `rel_path`, with byte spans; empty when
/// the language has no grammar.
#[must_use]
pub fn file_definitions(rel_path: &str, source_bytes: &[u8]) -> Vec<RawDef> {
    let lang = Lang::from_path(std::path::Path::new(rel_path));
    let lang = lang
        .semantics()
        .refine_dialect(lang, rel_path, source_bytes);
    let Some((language, query)) = LanguageRegistry.get(lang) else {
        return Vec::new();
    };
    parse_file_content(source_bytes, language, query, lang, rel_path).defs
}

/// Lines where `name` is defined in `source_bytes`, by the tags query of `rel_path`'s language.
#[must_use]
pub fn definition_lines(rel_path: &str, source_bytes: &[u8], name: &str) -> Vec<u32> {
    file_definitions(rel_path, source_bytes)
        .into_iter()
        .filter(|d| d.name == name)
        .map(|d| d.line)
        .collect()
}

/// Ingest crawled files in parallel and assemble IngestResult.
pub fn ingest_crawled_files(
    files: &[CrawledFile],
    registry: &LanguageRegistry,
    interner: &Interner,
) -> IngestResult {
    let parsed_files: Vec<Option<(Lang, FileParseResult)>> = files
        .par_iter()
        .map(|file| {
            let source_bytes = fs::read(&file.path).ok()?;
            if crate::crawler::is_binary_buffer(&source_bytes) {
                return None;
            }
            let eff_lang =
                file.lang
                    .semantics()
                    .refine_dialect(file.lang, &file.rel_path, &source_bytes);
            let (language, query) = registry.get(eff_lang)?;
            Some((
                eff_lang,
                parse_file_content(&source_bytes, language, query, eff_lang, &file.rel_path),
            ))
        })
        .collect();

    let processed: Vec<(FileInfo, FileIndex)> = (0..files.len())
        .into_par_iter()
        .zip(parsed_files)
        .map(|(file_id, parsed)| {
            let file = &files[file_id];
            let file_id = file_id as u32;
            let (eff_lang, parsed) = match parsed {
                Some((l, p)) => (l, p),
                None => (
                    file.lang,
                    FileParseResult {
                        byte_size: file.byte_size,
                        ..FileParseResult::default()
                    },
                ),
            };
            let file_info = FileInfo {
                id: file_id,
                path: interner.intern(&file.rel_path),
                lang: eff_lang,
                line_count: parsed.line_count,
                byte_size: parsed.byte_size,
            };
            let mut eff_file = file.clone();
            eff_file.lang = eff_lang;
            (file_info, index_file(parsed, &eff_file, file_id, interner))
        })
        .collect();

    let mut ingest = IngestResult::new();
    ingest.files.reserve(files.len());
    for (file_info, index) in processed {
        append_file(&mut ingest, file_info, index);
    }
    retain_accessor_reads(&mut ingest);
    ingest.import_aliases = files
        .iter()
        .filter_map(|f| {
            let name = f.rel_path.rsplit('/').next()?;
            crate::languages::ALL_LANGS
                .iter()
                .any(|l| l.semantics().import_alias_manifests().contains(&name))
                .then(|| fs::read_to_string(&f.path).ok())?
                .map(|content| crate::languages::import_aliases_in(&f.rel_path, &content))
        })
        .flatten()
        .collect();
    ingest
}

/// Ingest in-memory source files directly without filesystem I/O.
pub fn ingest_in_memory_files(
    files: &[(&str, Lang, &[u8])],
    registry: &LanguageRegistry,
    interner: &Interner,
) -> IngestResult {
    let mut ingest = IngestResult::new();

    for (file_id, &(rel_path, lang, source_bytes)) in files.iter().enumerate() {
        let file_id = file_id as u32;
        let eff_lang = lang
            .semantics()
            .refine_dialect(lang, rel_path, source_bytes);
        let parsed = registry.get(eff_lang).map(|(language, query)| {
            parse_file_content(source_bytes, language, query, eff_lang, rel_path)
        });

        let parsed = parsed.unwrap_or_else(|| FileParseResult {
            byte_size: source_bytes.len() as u64,
            ..FileParseResult::default()
        });
        let file_info = FileInfo {
            id: file_id,
            path: interner.intern(rel_path),
            lang: eff_lang,
            line_count: parsed.line_count,
            byte_size: parsed.byte_size,
        };
        let dummy_crawled = CrawledFile {
            path: std::path::PathBuf::from(rel_path),
            rel_path: rel_path.to_string(),
            lang: eff_lang,
            byte_size: source_bytes.len() as u64,
        };
        let index = index_file(parsed, &dummy_crawled, file_id, interner);
        append_file(&mut ingest, file_info, index);
    }
    retain_accessor_reads(&mut ingest);

    ingest.import_aliases = files
        .iter()
        .flat_map(|&(path, _, src)| {
            crate::languages::import_aliases_in(path, &String::from_utf8_lossy(src))
        })
        .collect();
    ingest
}

/// One file's symbols and references, with node ids local to the file (starting at 0).
pub struct FileIndex {
    pub symbols: Vec<Symbol>,
    pub references: Vec<Reference>,
    pub foreign_bindings: Vec<ForeignBinding>,
    pub callable_bindings: Vec<CallableBinding>,
    pub implementations: Vec<crate::domain::Implementation>,
}

pub fn index_file(
    parsed: FileParseResult,
    file: &CrawledFile,
    file_id: u32,
    interner: &Interner,
) -> FileIndex {
    let mut next_id: NodeId = 0;
    let mut symbols = build_file_symbols(parsed.defs, file, file_id, interner, &mut next_id);
    let mut references = build_file_references(parsed.refs, file, file_id, &symbols, interner);
    let callable_bindings =
        intern_callable_bindings(parsed.callable_bindings, &symbols, file_id, interner);
    assign_module_scope_owner(
        &mut symbols,
        &mut references,
        file,
        file_id,
        interner,
        &mut next_id,
    );
    let foreign_bindings = intern_foreign_bindings(parsed.foreign_exports, file_id, interner);
    let implementations = intern_implementations(parsed.implementations, file_id, interner);
    FileIndex {
        symbols,
        references,
        foreign_bindings,
        callable_bindings,
        implementations,
    }
}

fn intern_foreign_bindings(
    exports: Vec<crate::languages::ForeignExport>,
    file_id: u32,
    interner: &Interner,
) -> Vec<ForeignBinding> {
    exports
        .into_iter()
        .map(|export| ForeignBinding {
            file_id,
            exported: interner.intern(&export.exported),
            target: interner.intern(&export.target),
            target_scope: export.target_scope.map(|scope| interner.intern(&scope)),
            caller: export.caller,
        })
        .collect()
}

fn intern_callable_bindings(
    bindings: Vec<crate::languages::CallableBinding>,
    symbols: &[Symbol],
    file_id: u32,
    interner: &Interner,
) -> Vec<CallableBinding> {
    bindings
        .into_iter()
        .map(|binding| {
            let target = binding.target.as_deref().map(|value| {
                value
                    .rsplit_once("::")
                    .map_or((None, value), |(qualifier, name)| (Some(qualifier), name))
            });
            CallableBinding {
                file_id,
                from_symbol: super::ownership::find_enclosing_symbol(symbols, binding.start_byte),
                start_byte: binding.start_byte as u32,
                owner: interner.intern(&binding.owner),
                field: interner.intern(&binding.field),
                target: target.map(|(_, name)| interner.intern(name)),
                target_qualifier: target
                    .and_then(|(qualifier, _)| qualifier)
                    .map(|value| interner.intern(value)),
            }
        })
        .collect()
}

fn intern_implementations(
    implementations: Vec<crate::languages::Implementation>,
    file_id: u32,
    interner: &Interner,
) -> Vec<crate::domain::Implementation> {
    implementations
        .into_iter()
        .map(|implementation| {
            let trait_path = implementation.trait_path.as_deref().map(|value| {
                value
                    .rsplit_once("::")
                    .map_or((None, value), |(qualifier, name)| (Some(qualifier), name))
            });
            crate::domain::Implementation {
                file_id,
                subject: interner.intern(&implementation.subject),
                trait_name: trait_path.map(|(_, name)| interner.intern(name)),
                trait_qualifier: trait_path
                    .and_then(|(qualifier, _)| qualifier)
                    .map(|value| interner.intern(value)),
                subject_bound: implementation
                    .subject_bound
                    .as_deref()
                    .map(|value| interner.intern(value)),
                generic_subject: implementation.generic_subject,
                start_byte: implementation.start_byte as u32,
                end_byte: implementation.end_byte as u32,
            }
        })
        .collect()
}

/// Drops member reads that name no accessor in the corpus, so plain property reads cost nothing.
fn retain_accessor_reads(ingest: &mut IngestResult) {
    let accessors: std::collections::HashSet<lasso::Spur> = ingest
        .symbols
        .iter()
        .filter(|s| s.accessor)
        .map(|s| s.name)
        .collect();
    ingest
        .references
        .retain(|r| r.role != crate::domain::RefRole::Access || accessors.contains(&r.callee_name));
}

/// Append a file, rebasing its file-local node ids onto the global symbol table.
fn append_file(ingest: &mut IngestResult, file_info: FileInfo, mut index: FileIndex) {
    let offset = ingest.symbols.len() as NodeId;
    for s in &mut index.symbols {
        s.id += offset;
    }
    for r in &mut index.references {
        r.from_symbol += offset;
    }
    for binding in &mut index.callable_bindings {
        if binding.from_symbol != crate::domain::NO_NODE {
            binding.from_symbol += offset;
        }
    }
    ingest.files.push(file_info);
    ingest.symbols.append(&mut index.symbols);
    ingest.references.append(&mut index.references);
    ingest.foreign_bindings.append(&mut index.foreign_bindings);
    ingest
        .callable_bindings
        .append(&mut index.callable_bindings);
    ingest.implementations.append(&mut index.implementations);
}

pub fn build_file_symbols(
    defs: Vec<RawDef>,
    file: &CrawledFile,
    file_id: u32,
    interner: &Interner,
    next_node_id: &mut NodeId,
) -> Vec<Symbol> {
    let mut file_symbols = Vec::with_capacity(defs.len());
    for def in defs {
        let id = *next_node_id;
        *next_node_id += 1;
        let name = interner.intern(&def.name);
        let canon = match &def.scope {
            Some(sc) => interner.intern(&format!("{}::{}::{}", file.rel_path, sc, def.name)),
            None => interner.intern(&format!("{}::{}", file.rel_path, def.name)),
        };
        let mut sym = Symbol::new(id, name, canon, def.kind, file.lang, file_id, def.line);
        if let Some(sc) = &def.scope {
            sym.scope = Some(interner.intern(sc));
        }
        sym.sig_start_byte = def.start_byte as u32;
        sym.doc_len = def.doc_len as u32;
        sym.sig_end_byte = if def.body_byte > def.start_byte {
            def.body_byte as u32
        } else {
            def.end_byte as u32
        };
        sym.end_byte = def.end_byte as u32;
        sym.loc = def.loc.max(1);
        sym.cx = def.metrics.cx;
        sym.ccx = def.metrics.ccx;
        sym.max_nest = def.metrics.max_nest;
        sym.params = def.params;
        sym.humps = def.metrics.humps;
        sym.deep = def.metrics.deep;
        sym.ev = def.metrics.ev;
        sym.binding = def.binding;
        sym.contract_member = def.contract_member;
        sym.takes_self = def.takes_self;
        sym.accessor = def.accessor;
        sym.registered = def.registered;
        file_symbols.push(sym);
    }
    file_symbols
}
