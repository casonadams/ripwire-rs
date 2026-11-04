use ripwire_rs::metrics::nonlocal::{NonlocalFunction, find_nonlocal_state};
use ripwire_rs::{Graph, Interner, Lang, LanguageRegistry, ingest_in_memory_files};

pub fn state_report(lang: Lang, files: &[(&str, &str)]) -> Vec<NonlocalFunction> {
    let interner = Interner::new();
    let registry = LanguageRegistry::new().unwrap();
    let sources: Vec<(&str, Lang, &[u8])> = files
        .iter()
        .map(|(path, src)| (*path, lang, src.as_bytes()))
        .collect();
    let ingest = ingest_in_memory_files(&sources, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    find_nonlocal_state(&ingest, &graph, &interner)
}

pub fn function<'a>(report: &'a [NonlocalFunction], name: &str) -> Option<&'a NonlocalFunction> {
    report.iter().find(|f| f.sym_name == name)
}
