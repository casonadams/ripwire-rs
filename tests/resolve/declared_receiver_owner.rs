use crate::support::index::index_files;
use ripwire_rs::{Graph, IngestResult, Interner, Lang};

const GRAPH: &[u8] = b"pub struct Graph {\n    nodes: usize,\n}\n\nimpl Graph {\n    pub fn in_degree(&self) -> usize {\n        self.nodes\n    }\n}\n";
const STATE: &[u8] = b"use crate::graph::Graph;\n\npub struct State {\n    pub graph: Graph,\n}\n";
const REPORT: &[u8] = b"use crate::state::State;\n\npub fn degree(state: &State) -> usize {\n    state.graph.in_degree()\n}\n";
const ALIAS: &[u8] = b"type Graph = std::collections::HashMap<u32, u32>;\n";

/// The `name@file` of every callee of the function `degree`.
fn degree_callees(ingest: &IngestResult, graph: &Graph, interner: &Interner) -> Vec<String> {
    let from = ingest
        .symbols
        .iter()
        .find(|s| interner.resolve(&s.name) == "degree")
        .expect("caller is indexed");
    graph
        .callees(from.id)
        .map(|(to, _)| {
            let sym = &ingest.symbols[to as usize];
            let file = interner.resolve(&ingest.files[sym.file_id as usize].path);
            format!("{}@{file}", interner.resolve(&sym.name))
        })
        .collect()
}

/// `state.graph.in_degree()` reaches the owner through the field, without an import of `Graph` in the caller.
/// The field's type is named in the file that declares the field, so a second declaration of `Graph` elsewhere in
/// the tree does not make the call ambiguous: the declaring file's import selects the owner.
#[test]
fn field_typed_receiver_names_its_type_through_the_declaring_file() {
    let (ingest, graph, interner) = index_files(&[
        ("src/graph/mod.rs", Lang::Rust, GRAPH),
        ("src/state.rs", Lang::Rust, STATE),
        ("src/report/mod.rs", Lang::Rust, REPORT),
    ]);
    assert_eq!(
        degree_callees(&ingest, &graph, &interner),
        vec!["in_degree@src/graph/mod.rs".to_string()]
    );
    let (ingest, graph, interner) = index_files(&[
        ("src/graph/mod.rs", Lang::Rust, GRAPH),
        ("tests/git/history.rs", Lang::Rust, ALIAS),
        ("src/state.rs", Lang::Rust, STATE),
        ("src/report/mod.rs", Lang::Rust, REPORT),
    ]);
    assert_eq!(
        degree_callees(&ingest, &graph, &interner),
        vec!["in_degree@src/graph/mod.rs".to_string()]
    );
}
