use ripwire_rs::query::retrieval::call_lego;
use ripwire_rs::{Interner, Lang, LanguageRegistry, ingest_in_memory_files};

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
