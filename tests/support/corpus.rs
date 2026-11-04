use crate::index_support::index;
use ripwire_rs::{Graph, IngestResult, Interner, Lang};

/// An in-memory corpus indexed and graphed for one language.
pub struct Fixture {
    pub interner: Interner,
    pub ingest: IngestResult,
    pub graph: Graph,
}

pub fn fixture(lang: Lang, files: &[(&str, &str)]) -> Fixture {
    let sources: Vec<(&str, Lang, &[u8])> = files
        .iter()
        .map(|(path, src)| (*path, lang, src.as_bytes()))
        .collect();
    let (ingest, graph, interner) = index(&sources);
    Fixture {
        interner,
        ingest,
        graph,
    }
}
