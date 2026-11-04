use ripwire_rs::{Graph, IngestResult, Interner, Lang, LanguageRegistry, ingest_in_memory_files};

/// Indexes in-memory `files` and builds their call graph.
pub fn index(files: &[(&str, Lang, &[u8])]) -> (IngestResult, Graph, Interner) {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(files, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    (ingest, graph, interner)
}
