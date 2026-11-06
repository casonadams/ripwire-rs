use ripwire_rs::{
    Graph, IngestResult, Interner, Lang, LanguageRegistry, NodeId, ingest_in_memory_files,
};

/// The callees of `caller` in an in-memory corpus, each described by `describe`, sorted.
pub fn callees_described<D: Ord>(
    files: &[(&str, Lang, &[u8])],
    caller: &str,
    describe: impl Fn(&IngestResult, &Interner, NodeId) -> D,
) -> Vec<D> {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(files, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    let caller_id = ingest
        .symbols
        .iter()
        .find(|s| interner.resolve(&s.name) == caller)
        .unwrap_or_else(|| panic!("{caller} not indexed"))
        .id;
    let mut out: Vec<D> = graph
        .callees(caller_id)
        .map(|(id, _)| describe(&ingest, &interner, id))
        .collect();
    out.sort();
    out
}

/// A symbol as `file:name`.
fn file_and_name(ingest: &IngestResult, interner: &Interner, id: NodeId) -> String {
    let sym = &ingest.symbols[id as usize];
    let file = interner.resolve(&ingest.files[sym.file_id as usize].path);
    format!("{file}:{}", interner.resolve(&sym.name))
}

/// The callees of `caller` in an in-memory corpus as `file:name`, sorted.
pub fn callee_file_names(files: &[(&str, Lang, &[u8])], caller: &str) -> Vec<String> {
    callees_described(files, caller, file_and_name)
}
