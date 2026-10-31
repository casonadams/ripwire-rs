use super::harness::{GraphProtocolCase, owner_qualified_callees, sym, verify_protocol_case};
use ripwire_rs::query::retrieval::call_lego;
use ripwire_rs::{Interner, Lang, LanguageRegistry, SymKind, ingest_in_memory_files};

#[test]
fn test_cpp_graph_protocol_and_macro_gates() {
    verify_protocol_case(&GraphProtocolCase {
        label: "cpp",
        files: &[(
            "src/engine.cpp",
            Lang::Cpp,
            b"#include \"engine.h\"\n#define NOOP_MACRO(x)\nstatic const int MAX_THREADS = 8;\nconstexpr int kValidConst = 7;\nint mutable_global = 0;\nint uninitialized_global;\nclass BaseEngine {\npublic:\n    static constexpr int kClassConst = 9;\n    static int s_mutable;\n    int count;\n};\ntemplate<> class BaseEngine<int> {};\nclass RenderEngine : public BaseEngine {\npublic:\n    void render() {\n        std::lock_guard<std::mutex> guard(m);\n        std::move(0);\n        flushBuffers();\n    }\n};\nvoid flushBuffers() {\n    return;\n}\nTEST_CASE(\"suite\") {\n    flushBuffers();\n}\n",
        )],
        expected_syms: &[
            sym("MAX_THREADS", SymKind::Var, None, false),
            sym("kValidConst", SymKind::Var, None, false),
            sym("kClassConst", SymKind::Var, Some("BaseEngine"), false),
            sym("count", SymKind::Field, Some("BaseEngine"), false),
            sym("RenderEngine", SymKind::Class, None, true),
            sym("render", SymKind::Method, Some("RenderEngine"), true),
            sym("flushBuffers", SymKind::Function, None, true),
            sym("TEST_CASE", SymKind::Macro, None, true),
        ],
        rejected_syms: &["NOOP_MACRO", "mutable_global", "uninitialized_global", "s_mutable", "guard"],
        expected_edges: &[("render", "flushBuffers")],
        expect_external: true,
        expected_lego: Some(("BaseEngine", "RenderEngine")),
        expected_import: Some("engine.h"),
    });
}

#[test]
fn test_cpp_macro_blanking_and_attribute_macros() {
    let registry = LanguageRegistry::new().unwrap();
    let interner = Interner::new();
    let src = br#"
#define CALL_HELPER(x) do { real_helper(x); if (x) {} } while(0)
class Widget {
    REGISTER_WIDGET(Widget)
    [[noreturn]] RW_COLD_NOINLINE
    static void handlePanic(const char* file, int line) noexcept;
public:
    void work() {
        int v = static_cast<int>(1.0);
        CALL_HELPER(v);
    }
};
RW_COLD_NOINLINE
void Widget::handlePanic(const char* file, int line) noexcept {}
void real_helper(int x) {}
"#;
    let ingest = ingest_in_memory_files(&[("widget.cpp", Lang::Cpp, src)], &registry, &interner);
    let names: Vec<&str> = ingest.symbols.iter().map(|s| &interner[s.name]).collect();
    assert!(names.contains(&"Widget"));
    assert!(names.contains(&"work"));
    assert!(names.contains(&"handlePanic"));
    assert!(names.contains(&"CALL_HELPER"));
    let callees: Vec<&str> = ingest
        .references
        .iter()
        .map(|r| &interner[r.callee_name])
        .collect();
    assert!(callees.contains(&"real_helper"));
    assert!(!callees.contains(&"static_cast"));
    assert!(!callees.contains(&"if"));

    let lego = call_lego(&ingest, &interner, "Widget");
    let methods = lego["methods"].as_array().unwrap();
    let method_names: Vec<&str> = methods.iter().filter_map(|m| m["name"].as_str()).collect();
    assert!(method_names.contains(&"handlePanic"));
}

#[test]
fn cpp_inherited_and_typed_receivers_pick_the_right_class_method() {
    let source = "class Base {\n public:\n  void refill() {}\n};\n\
class Other {\n public:\n  void refill() {}\n};\n\
class Store {\n public:\n  void stash(int v) {}\n};\n\
class Cache {\n public:\n  void stash(int v) {}\n};\n\
class Sub : public Base {\n public:\n  void viaThis() { this->refill(); }\n  void bare() { refill(); }\n  void viaRef(Store& store) { store.stash(1); }\n  void viaPtr(Store* store) { store->stash(2); }\n};\n";
    let callees =
        |caller| owner_qualified_callees(&[("src/sub.cpp", Lang::Cpp, source.as_bytes())], caller);
    assert_eq!(callees("viaThis"), ["Base.refill"]);
    assert_eq!(callees("bare"), ["Base.refill"]);
    assert_eq!(callees("viaRef"), ["Store.stash"]);
    assert_eq!(callees("viaPtr"), ["Store.stash"]);
}

#[test]
fn cpp_destructor_bodies_attribute_their_calls() {
    let in_class = "static void release_all() {}\nclass Canvas {\npublic:\n  ~Canvas() { release_all(); }\n};\n";
    let out_of_line = "static void release_all() {}\nclass Canvas {\npublic:\n  void flush();\n};\nCanvas::~Canvas() { release_all(); }\n";
    for source in [in_class, out_of_line] {
        let callees = owner_qualified_callees(
            &[("src/canvas.cpp", Lang::Cpp, source.as_bytes())],
            "~Canvas",
        );
        assert_eq!(callees, [".release_all"]);
    }
}
