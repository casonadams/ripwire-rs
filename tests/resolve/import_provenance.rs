use ripwire_rs::{EdgeProv, Graph, Interner, Lang, LanguageRegistry, ingest_in_memory_files};

type Fixture<'a> = (&'a str, Lang, &'a [u8]);

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
    let rust_def: Fixture = ("src/b.rs", Lang::Rust, b"pub fn helper() {}\n");
    let py_def: Fixture = ("util.py", Lang::Python, b"def helper():\n    return 1\n");
    let cases: [([Fixture; 2], Option<EdgeProv>); 4] = [
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

#[test]
fn a_written_path_through_a_re_exporting_module_proves_the_edge() {
    let files: [(&str, Lang, &[u8]); 3] = [
        (
            "src/serialize/mod.rs",
            Lang::Rust,
            b"mod budget;\npub use budget::row_bytes;\n",
        ),
        (
            "src/serialize/budget.rs",
            Lang::Rust,
            b"pub fn row_bytes() {}\n",
        ),
        (
            "src/gate/report.rs",
            Lang::Rust,
            b"fn probe() { crate::serialize::row_bytes(); }\n",
        ),
    ];
    assert_eq!(
        edge_prov(&files, "probe", "row_bytes"),
        Some(EdgeProv::Import)
    );
}
