use super::harness::{GraphProtocolCase, sym, verify_protocol_case};
use ripwire_rs::{Graph, Interner, Lang, LanguageRegistry, SymKind, ingest_in_memory_files};

#[test]
fn test_zig_graph_protocol() {
    verify_protocol_case(&GraphProtocolCase {
        label: "zig",
        files: &[(
            "src/main.zig",
            Lang::Zig,
            b"const Engine = struct {\n    pub fn start() void {\n        std.print();\n        helper();\n    }\n};\nconst Mode = enum { fast, slow };\nconst Payload = union { raw: u32 };\npub fn helper() void {\n    return;\n}\n",
        )],
        expected_syms: &[
            sym("Engine", SymKind::Class, None, false),
            sym("Mode", SymKind::Var, None, false),
            sym("Payload", SymKind::Class, None, false),
            sym("start", SymKind::Function, Some("Engine"), true),
            sym("helper", SymKind::Function, None, true),
        ],
        expected_edges: &[("start", "helper")],
        expect_external: true,
        ..Default::default()
    });
}

/// Callee symbols of `caller`, as `file:name`, across an in-memory multi-file ingest.
fn callee_locations(files: &[(&str, Lang, &[u8])], caller: &str) -> Vec<String> {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(files, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    let caller = ingest
        .symbols
        .iter()
        .find(|s| interner.resolve(&s.name) == caller)
        .unwrap_or_else(|| panic!("{caller} not indexed"))
        .id;
    let mut out: Vec<String> = graph
        .callees(caller)
        .map(|(id, _)| {
            let sym = &ingest.symbols[id as usize];
            let file = interner.resolve(&ingest.files[sym.file_id as usize].path);
            format!("{file}:{}", interner.resolve(&sym.name))
        })
        .collect();
    out.sort();
    out
}

#[test]
fn zig_return_type_is_not_a_definition_and_methods_scope_to_their_struct() {
    let files: &[(&str, Lang, &[u8])] = &[(
        "src/store.zig",
        Lang::Zig,
        b"pub const Store = struct {\n    pub fn push(self: *Store, v: u32) void {\n        _ = v;\n    }\n    pub fn len(self: *const Store) usize {\n        _ = self;\n        return 0;\n    }\n};\npub fn make() Store {\n    return .{};\n}\n",
    )];
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let ingest = ingest_in_memory_files(files, &registry, &interner);
    let named = |name: &str| {
        ingest
            .symbols
            .iter()
            .filter(|s| interner.resolve(&s.name) == name)
            .collect::<Vec<_>>()
    };
    assert!(
        named("usize").is_empty(),
        "return type `usize` must not define a function"
    );
    let store = named("Store");
    assert_eq!(
        store.len(),
        1,
        "`Store` is the struct only, not the return type of `make`"
    );
    assert_eq!(store[0].kind, SymKind::Class);
    let push = named("push");
    assert_eq!(
        push[0]
            .scope
            .map(|s| interner.resolve(&s).to_string())
            .as_deref(),
        Some("Store")
    );
}

#[test]
fn zig_import_binding_pins_qualified_calls_into_the_imported_file() {
    let files: &[(&str, Lang, &[u8])] = &[
        (
            "src/store.zig",
            Lang::Zig,
            b"const util = @import(\"util.zig\");\npub fn push(v: u32) void {\n    util.log(v);\n}\n",
        ),
        ("src/util.zig", Lang::Zig, b"pub fn log(v: u32) void {\n    _ = v;\n}\n"),
        ("src/other.zig", Lang::Zig, b"pub fn log(v: u32) void {\n    _ = v;\n}\n"),
    ];
    assert_eq!(callee_locations(files, "push"), ["src/util.zig:log"]);
}
