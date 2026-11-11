use ripwire_rs::{EdgeProv, Graph, Interner, Lang, LanguageRegistry, ingest_in_memory_files};

/// The provenance of the edge from `caller` to `callee` in an in-memory corpus.
fn edge_prov(files: &[(&str, Lang, &[u8])], caller: &str, callee: &str) -> Option<EdgeProv> {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(files, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    let id = |name: &str| {
        ingest
            .symbols
            .iter()
            .find(|s| interner.resolve(&s.name) == name)
            .unwrap_or_else(|| panic!("{name} not indexed"))
            .id
    };
    graph.edge_prov(id(caller), id(callee))
}

#[test]
fn a_written_import_proves_the_only_definition_of_a_called_name() {
    let rust_def: (&str, Lang, &[u8]) = ("src/b.rs", Lang::Rust, b"pub fn helper() {}\n");
    let py_def: (&str, Lang, &[u8]) = ("util.py", Lang::Python, b"def helper():\n    return 1\n");
    let cases: [([(&str, Lang, &[u8]); 2], Option<EdgeProv>); 4] = [
        (
            [
                rust_def,
                (
                    "src/a.rs",
                    Lang::Rust,
                    b"use crate::b::helper;\npub fn run() { helper(); }\n",
                ),
            ],
            Some(EdgeProv::Import),
        ),
        (
            [
                rust_def,
                ("src/a.rs", Lang::Rust, b"pub fn run() { helper(); }\n"),
            ],
            Some(EdgeProv::Name),
        ),
        (
            [
                py_def,
                (
                    "main.py",
                    Lang::Python,
                    b"from util import helper\ndef run():\n    helper()\n",
                ),
            ],
            Some(EdgeProv::Import),
        ),
        (
            [
                py_def,
                ("main.py", Lang::Python, b"def run():\n    helper()\n"),
            ],
            Some(EdgeProv::Name),
        ),
    ];
    for (files, expected) in cases {
        assert_eq!(edge_prov(&files, "run", "helper"), expected, "{files:?}");
    }
}
