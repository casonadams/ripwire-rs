use ripwire_rs::gate::baseline::{
    QualitySnapshot, parse_snapshot_from_str, serialize_snapshot_to_string,
};

#[test]
fn test_snapshot_roundtrip_serialization() {
    let mut snap = QualitySnapshot {
        head_sha: "abcdef1234567890abcdef1234567890abcdef12".to_string(),
        dirty: true,
        absorbed: 2,
        ..Default::default()
    };
    snap.ccx_by_sym.insert("src/calc.rs::add".to_string(), 3);
    snap.cx_by_sym.insert("src/calc.rs::add".to_string(), 2);
    snap.loc_by_sym.insert("src/calc.rs::add".to_string(), 10);
    snap.nest_by_sym.insert("src/calc.rs::add".to_string(), 1);
    snap.params_by_sym.insert("src/calc.rs::add".to_string(), 2);
    snap.dead_symbols.insert("src/calc.rs::unused".to_string());
    snap.public_api.insert("src/calc.rs::add".to_string());

    let text = serialize_snapshot_to_string(&snap);
    assert!(text.contains("head abcdef1234567890abcdef1234567890abcdef12"));
    assert!(text.contains("dirty 1"));
    assert!(text.contains("ccx src/calc.rs::add 3"));

    let parsed = parse_snapshot_from_str(&text);
    assert_eq!(parsed.head_sha, snap.head_sha);
    assert!(parsed.dirty);
    assert_eq!(parsed.absorbed, 2);
    assert_eq!(parsed.ccx_by_sym["src/calc.rs::add"], 3);
    assert_eq!(parsed.params_by_sym["src/calc.rs::add"], 2);
    assert!(parsed.dead_symbols.contains("src/calc.rs::unused"));
    assert!(parsed.public_api.contains("src/calc.rs::add"));
}

fn dead_symbols(files: &[(&str, ripwire_rs::Lang, &str)]) -> Vec<String> {
    use ripwire_rs::{Graph, Interner, LanguageRegistry, ingest_in_memory_files};
    let registry = LanguageRegistry::new().unwrap_or_else(|e| panic!("registry: {e:?}"));
    let interner = Interner::new();
    let slices: Vec<_> = files
        .iter()
        .map(|(p, l, s)| (*p, *l, s.as_bytes()))
        .collect();
    let ingest = ingest_in_memory_files(&slices, &registry, &interner);
    let graph = Graph::build(&ingest, &interner);
    let snap = ripwire_rs::gate::baseline::compute_snapshot(&ingest, &graph, &interner, "head");
    let mut dead: Vec<String> = snap.dead_symbols.into_iter().collect();
    dead.sort_unstable();
    dead
}

#[test]
fn test_snapshot_dead_symbols_are_unreferenced_callables_in_any_language() {
    use ripwire_rs::Lang::{Rust, TypeScript};
    let rust = "fn orphan() -> u32 {\n    1\n}\n\nfn main() {}\n\nstruct Store;\n\nimpl Store {\n    fn size(&self) -> u32 {\n        2\n    }\n}\n\nstruct Shown;\n\nimpl std::fmt::Display for Shown {\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n        write!(f, \"x\")\n    }\n}\n";
    let ts = "export class Client {\n  constructor() {}\n}\n\nexport function unused(): number {\n  return 1;\n}\n\nexport const tool = {\n  execute() {\n    return 2;\n  },\n};\n";
    let dead = dead_symbols(&[
        ("src/store.rs", Rust, rust),
        ("src/client.ts", TypeScript, ts),
        (
            "tests/store_test.rs",
            Rust,
            "#[test]\nfn store_is_small() {\n    assert!(true);\n}\n",
        ),
    ]);
    assert_eq!(
        dead,
        [
            "src/client.ts::unused",
            "src/store.rs::Store::size",
            "src/store.rs::orphan"
        ]
    );
}

#[test]
fn test_snapshot_exempts_only_methods_a_supertype_dispatches_to() {
    use ripwire_rs::Lang::{Python, Rust, TypeScript};
    let rust = "struct Shown;\n\nimpl std::fmt::Display for Shown {\n    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n        write!(f, \"x\")\n    }\n}\n\nimpl Shown {\n    fn stale(&self) -> u32 {\n        1\n    }\n}\n";
    let ts = "interface Store {\n  load(): number;\n}\n\nexport class Disk implements Store {\n  load(): number {\n    return 1;\n  }\n\n  unused(): number {\n    return 2;\n  }\n}\n";
    let py = "from base import Handler\n\n\nclass Echo(Handler):\n    def handle(self):\n        return 1\n\n    def __repr__(self):\n        return \"echo\"\n\n\nclass Plain:\n    def orphan(self):\n        return 2\n";
    let dead = dead_symbols(&[
        ("src/shown.rs", Rust, rust),
        ("src/disk.ts", TypeScript, ts),
        ("src/echo.py", Python, py),
    ]);
    assert_eq!(
        dead,
        [
            "src/disk.ts::Disk::unused",
            "src/echo.py::Plain::orphan",
            "src/shown.rs::Shown::stale"
        ]
    );
}

#[test]
fn test_snapshot_exempts_registered_and_protocol_callables() {
    use ripwire_rs::Lang::{Python, TypeScript};
    let py = "from flask import Blueprint\n\nbp = Blueprint(\"blog\", __name__)\n\n\n@bp.route(\"/\")\ndef index():\n    return \"ok\"\n\n\n@staticmethod\ndef helper():\n    return 1\n\n\ndef __getattr__(name):\n    return name\n";
    let ts = "export class Api {\n  @Get(\"/\")\n  list(): number {\n    return 1;\n  }\n\n  stale(): number {\n    return 2;\n  }\n}\n";
    let dead = dead_symbols(&[("src/blog.py", Python, py), ("src/api.ts", TypeScript, ts)]);
    assert_eq!(dead, ["src/api.ts::Api::stale", "src/blog.py::helper"]);
}
