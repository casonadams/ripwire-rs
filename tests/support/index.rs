use ripwire_rs::{Graph, IngestResult, Interner, Lang, LanguageRegistry, ingest_in_memory_files};

/// Indexes in-memory `files` and builds their call graph.
pub fn index(files: &[(&str, Lang, &[u8])]) -> (IngestResult, Graph, Interner) {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(files, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    (ingest, graph, interner)
}

/// Indexes `source` as one TypeScript module at `src/mod.ts` and hands the result to `f`.
pub fn with_typescript_module<T>(source: &str, f: impl FnOnce(&IngestResult, &Interner) -> T) -> T {
    let (ingest, _, interner) = index(&[("src/mod.ts", Lang::TypeScript, source.as_bytes())]);
    f(&ingest, &interner)
}
