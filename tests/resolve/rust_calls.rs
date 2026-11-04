use crate::index_support::index;
use ripwire_rs::{Graph, IngestResult, Interner, Lang};

/// The `name@file` of every callee of the first symbol named `caller`, sorted. A struct literal or a type
/// mention is a callee too, so `make` reaches its `Opts` type as well as the `default` it calls.
fn callees(ingest: &IngestResult, graph: &Graph, interner: &Interner, caller: &str) -> Vec<String> {
    let from = ingest
        .symbols
        .iter()
        .find(|s| interner.resolve(&s.name) == caller)
        .expect("caller is indexed");
    let mut out: Vec<String> = graph
        .callees(from.id)
        .map(|(to, _)| {
            let sym = &ingest.symbols[to as usize];
            let file = interner.resolve(&ingest.files[sym.file_id as usize].path);
            format!("{}@{file}", interner.resolve(&sym.name))
        })
        .collect();
    out.sort();
    out
}

#[test]
fn struct_update_default_binds_to_the_literal_types_own_impl() {
    let (ingest, graph, interner) = index(&[(
        "src/opts.rs",
        Lang::Rust,
        b"pub struct Opts {\n    pub a: u32,\n    pub b: u32,\n}\n\nimpl Default for Opts {\n    fn default() -> Self {\n        Self { a: 1, b: 2 }\n    }\n}\n\npub fn make() -> Opts {\n    Opts {\n        a: 3,\n        ..Default::default()\n    }\n}\n",
    )]);
    assert_eq!(
        callees(&ingest, &graph, &interner, "make"),
        vec![
            "Opts@src/opts.rs".to_string(),
            "default@src/opts.rs".to_string(),
        ]
    );
}

#[test]
fn struct_update_default_of_a_derived_type_does_not_bind_to_another_types_impl() {
    let (ingest, graph, interner) = index(&[
        (
            "src/a.rs",
            Lang::Rust,
            b"#[derive(Default)]\npub struct Opts {\n    pub a: u32,\n}\n\npub fn make() -> Opts {\n    Opts {\n        a: 3,\n        ..Default::default()\n    }\n}\n",
        ),
        (
            "src/b.rs",
            Lang::Rust,
            b"pub struct Opts {\n    pub a: u32,\n}\n\nimpl Default for Opts {\n    fn default() -> Self {\n        Self { a: 1 }\n    }\n}\n",
        ),
    ]);
    assert_eq!(
        callees(&ingest, &graph, &interner, "make"),
        vec!["Opts@src/a.rs".to_string()]
    );
}

#[test]
fn call_inside_macro_arguments_is_counted_with_its_own_arguments() {
    let (ingest, graph, interner) = index(&[(
        "src/lib.rs",
        Lang::Rust,
        b"fn one(x: u32) -> u32 {\n    x\n}\n\nfn outer(v: Vec<u32>, a: u32, b: u32) -> u32 {\n    v.len() as u32 + a + b\n}\n\npub fn caller() -> u32 {\n    outer(vec![one(1)], 2, 3)\n}\n",
    )]);
    assert_eq!(
        callees(&ingest, &graph, &interner, "caller"),
        vec!["one@src/lib.rs".to_string(), "outer@src/lib.rs".to_string()]
    );
}

#[test]
fn self_recursive_call_leaves_no_self_edge() {
    let (ingest, graph, interner) = index(&[(
        "src/walk.rs",
        Lang::Rust,
        b"pub fn walk(n: u32) -> u32 {\n    if n == 0 {\n        0\n    } else {\n        walk(n - 1)\n    }\n}\n",
    )]);
    assert!(callees(&ingest, &graph, &interner, "walk").is_empty());
}

#[test]
fn member_call_on_another_receiver_is_not_bound_to_its_own_caller() {
    let (ingest, graph, interner) = index(&[(
        "src/kind.rs",
        Lang::Rust,
        b"pub struct Lang;\n\npub struct Kind;\n\nimpl Kind {\n    pub fn tag(&self, lang: &Lang) -> u8 {\n        lang.tag()\n    }\n}\n",
    )]);
    assert!(callees(&ingest, &graph, &interner, "tag").is_empty());
}

#[test]
fn std_named_members_bind_only_on_a_typed_in_tree_receiver() {
    let (ingest, graph, interner) = index(&[(
        "src/table.rs",
        Lang::Rust,
        b"pub struct Table {\n    rows: Vec<u8>,\n}\n\nimpl Table {\n    pub fn get(&self) -> u8 {\n        self.rows.len() as u8\n    }\n}\n\npub fn read(t: &Table) -> u8 {\n    t.get()\n}\n\npub fn size(v: &[u8]) -> usize {\n    v.len()\n}\n",
    )]);
    assert_eq!(
        callees(&ingest, &graph, &interner, "read"),
        vec!["get@src/table.rs".to_string()]
    );
    assert!(callees(&ingest, &graph, &interner, "size").is_empty());
    assert!(callees(&ingest, &graph, &interner, "get").is_empty());
}
