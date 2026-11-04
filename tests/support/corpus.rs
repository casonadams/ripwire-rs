use ripwire_rs::{Graph, IngestResult, Interner, Lang, LanguageRegistry, ingest_in_memory_files};

/// An in-memory corpus indexed and graphed for one language.
pub struct Fixture {
    pub interner: Interner,
    pub ingest: IngestResult,
    pub graph: Graph,
}

pub fn fixture(lang: Lang, files: &[(&str, &str)]) -> Fixture {
    let interner = Interner::new();
    let registry = LanguageRegistry::new().unwrap();
    let sources: Vec<(&str, Lang, &[u8])> = files
        .iter()
        .map(|(path, src)| (*path, lang, src.as_bytes()))
        .collect();
    let ingest = ingest_in_memory_files(&sources, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    Fixture {
        interner,
        ingest,
        graph,
    }
}
