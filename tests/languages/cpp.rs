use super::harness::owner_qualified_callees;
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

#[test]
fn cpp_qualified_fields_type_their_receivers_and_std_members_stay_external() {
    let files: &[(&str, Lang, &[u8])] = &[
        (
            "include/ring.h",
            Lang::Cpp,
            b"namespace details {\nclass Ring {\npublic:\n  void push_back(int v);\n  bool empty() const;\n};\n}\nclass Stack {\npublic:\n  void push_back(int v);\n};\n",
        ),
        (
            "include/null_mutex.h",
            Lang::Cpp,
            b"class NullMutex {\npublic:\n  void lock() {}\n  void unlock() {}\n};\n",
        ),
        (
            "src/tracer.cpp",
            Lang::Cpp,
            b"#include \"ring.h\"\n#include \"null_mutex.h\"\nclass Tracer {\n  details::Ring messages_;\n  std::atomic<int> level_;\n  NullMutex mutex_;\npublic:\n  void push(int v) { messages_.push_back(v); }\n  void push_this(int v) { this->messages_.push_back(v); }\n  void raise() { level_.load(); mutex_.lock(); }\n};\nvoid fill() { std::vector<int> v; v.push_back(1); }\n",
        ),
    ];
    let owned = |caller: &str| owner_qualified_callees(files, caller);
    let ring_push = |callees: &[String]| callees.iter().any(|c| c.ends_with("Ring.push_back"));
    assert!(ring_push(&owned("push")), "{:?}", owned("push"));
    assert!(ring_push(&owned("push_this")), "{:?}", owned("push_this"));
    assert!(!owned("push").iter().any(|c| c == "Stack.push_back"));
    assert!(!owned("fill").iter().any(|c| c.ends_with("push_back")));
    assert!(
        owned("raise").iter().any(|c| c == "NullMutex.lock"),
        "{:?}",
        owned("raise")
    );
    assert!(
        !owned("raise").iter().any(|c| c.ends_with(".load")),
        "{:?}",
        owned("raise")
    );
}

#[test]
fn cpp_fields_and_aliases_declared_in_headers_type_receivers_in_other_files() {
    let files: &[(&str, Lang, &[u8])] = &[
        (
            "src/tracer.cpp",
            Lang::Cpp,
            b"#include \"ring.h\"\n#include \"tracer.h\"\nvoid Tracer::push_one(int v) { messages_.push_back(v); }\nvoid Tracer::push_two(int v) { this->messages_.push_back(v); }\n",
        ),
        (
            "include/ring.h",
            Lang::Cpp,
            b"namespace details {\nclass Ring {\npublic:\n  void push_back(int v) {}\n};\n}\n",
        ),
        (
            "include/tracer.h",
            Lang::Cpp,
            b"#include \"ring.h\"\nclass Tracer {\n  details::Ring messages_;\npublic:\n  void push_one(int v);\n  void push_two(int v);\n};\n",
        ),
        (
            "src/sv.cpp",
            Lang::Cpp,
            b"#include \"../include/sv.h\"\nunsigned measure(const sv_t &s) { return s.size(); }\n",
        ),
        (
            "include/sv.h",
            Lang::Cpp,
            b"namespace fmt {\nclass basic_sv {\npublic:\n  unsigned size() const { return 0; }\n};\n}\nusing sv_t = fmt::basic_sv;\n",
        ),
    ];
    let owned = |caller: &str| owner_qualified_callees(files, caller);
    assert!(
        owned("push_one")
            .iter()
            .any(|c| c.ends_with("Ring.push_back")),
        "{:?}",
        owned("push_one")
    );
    assert!(
        owned("push_two")
            .iter()
            .any(|c| c.ends_with("Ring.push_back")),
        "{:?}",
        owned("push_two")
    );
    assert!(
        owned("measure")
            .iter()
            .any(|c| c.ends_with("basic_sv.size")),
        "{:?}",
        owned("measure")
    );
}

#[test]
fn cpp_template_class_declared_in_an_included_header_owns_its_receiver_calls() {
    let files: &[(&str, Lang, &[u8])] = &[
        (
            "src/main.cpp",
            Lang::Cpp,
            b"#include \"../include/box.h\"\nint use_box() { Box<int> b; return b.get(); }\nint use_plain(Plain& p) { return p.get(); }\n",
        ),
        (
            "include/box.h",
            Lang::Cpp,
            b"template <class T> class Box {\npublic:\n  T get() const { return v_; }\nprivate:\n  T v_;\n};\nclass Plain {\npublic:\n  int get() const { return 1; }\n};\n",
        ),
    ];
    let owned = |caller: &str| owner_qualified_callees(files, caller);
    assert!(
        owned("use_plain").iter().any(|c| c == "Plain.get"),
        "{:?}",
        owned("use_plain")
    );
    assert!(
        owned("use_box").iter().any(|c| c == "Box.get"),
        "{:?}",
        owned("use_box")
    );
}

#[test]
fn cpp_bodyless_methods_bind_through_typed_receivers_to_their_definition_or_declaration() {
    use crate::support::callees::callees_described;
    let describe = |ingest: &ripwire_rs::IngestResult, interner: &Interner, id: u32| {
        let sym = &ingest.symbols[id as usize];
        let owner = sym.scope.map_or("", |s| interner.resolve(&s));
        let body = if sym.has_body() { "def" } else { "decl" };
        format!("{owner}.{}#{body}", interner.resolve(&sym.name))
    };
    let files: &[(&str, Lang, &[u8])] = &[
        (
            "src/log.cpp",
            Lang::Cpp,
            b"#include \"../include/fmt.h\"\nvoid by_pointer(Formatter* f) { f->clone(); }\nvoid by_ref(Formatter& f) { f.clone(); }\nvoid by_patch(Patch* p) { p->clone(); }\nint use_draw(Formatter& f) { return f.draw(1); }\nint by_vector() { std::vector<int> v; return v.size(); }\n",
        ),
        (
            "include/fmt.h",
            Lang::Cpp,
            b"#pragma once\nclass Formatter {\npublic:\n  virtual Formatter* clone() const = 0;\n  virtual int size() const = 0;\n  int draw(int n);\n};\nclass Patch : public Formatter {\npublic:\n  Formatter* clone() const override { return nullptr; }\n};\n",
        ),
        (
            "src/fmt.cpp",
            Lang::Cpp,
            b"#include \"../include/fmt.h\"\nint Formatter::draw(int n) { return n; }\n",
        ),
    ];
    let callees = |caller: &str| callees_described(files, caller, describe);
    assert_eq!(callees("by_pointer"), ["Formatter.clone#decl"]);
    assert_eq!(callees("by_ref"), ["Formatter.clone#decl"]);
    assert_eq!(callees("by_patch"), ["Patch.clone#def"]);
    assert_eq!(callees("use_draw"), ["Formatter.draw#def"]);
    assert!(
        !callees("by_vector")
            .iter()
            .any(|c| c.starts_with("Formatter."))
    );
}

#[test]
fn cpp_smart_pointer_and_optional_receivers_take_their_pointee_except_wrapper_members() {
    let files: &[(&str, Lang, &[u8])] = &[
        (
            "src/registry.cpp",
            Lang::Cpp,
            b"#include \"../include/registry.h\"\nvoid Registry::reset_formatter() { formatter_->clone(); }\nvoid Registry::share() { shared_->clone(); }\nvoid Registry::drop_formatter() { formatter_.reset(); }\nstd::unique_ptr<Formatter> by_local() { std::unique_ptr<Formatter> f; return f->clone(); }\nint by_optional(std::optional<Formatter>& o) { return o->size(); }\nint by_vector() { std::vector<Formatter> v; return v.size(); }\n",
        ),
        (
            "include/registry.h",
            Lang::Cpp,
            b"#pragma once\n#include <memory>\nclass Formatter {\npublic:\n  virtual std::unique_ptr<Formatter> clone() const = 0;\n  virtual int size() const = 0;\n  void reset();\n};\nclass Registry {\n  std::unique_ptr<Formatter> formatter_;\n  std::shared_ptr<Formatter> shared_;\npublic:\n  void reset_formatter();\n  void share();\n  void drop_formatter();\n};\n",
        ),
    ];
    let expectations = [
        ("reset_formatter", "Formatter.clone", true),
        ("share", "Formatter.clone", true),
        ("by_local", "Formatter.clone", true),
        ("by_optional", "Formatter.size", true),
        ("drop_formatter", "Formatter.reset", false),
        ("by_vector", "Formatter.size", false),
    ];
    for (caller, callee, bound) in expectations {
        let owned = owner_qualified_callees(files, caller);
        assert_eq!(
            owned.iter().any(|c| c == callee),
            bound,
            "{caller}: {owned:?}"
        );
    }
}

#[test]
fn cpp_core_member_names_on_a_receiver_of_another_type_do_not_bind_to_an_in_tree_class_by_name() {
    let files: &[(&str, Lang, &[u8])] = &[
        (
            "src/logger-inl.h",
            Lang::Cpp,
            b"#include \"logger.h\"\nvoid Logger::swap(Logger& other) { name_.swap(other.name_); }\nLogger* Registry::fetch() { return default_logger_.get(); }\n",
        ),
        (
            "include/logger.h",
            Lang::Cpp,
            b"#pragma once\n#include <memory>\n#include <string>\nclass Logger {\n  std::string name_;\npublic:\n  void swap(Logger& other);\n  Logger* get();\n};\nclass Registry {\n  std::shared_ptr<Logger> default_logger_;\npublic:\n  Logger* fetch();\n};\n",
        ),
    ];
    for (caller, callee) in [("swap", "Logger.swap"), ("fetch", "Logger.get")] {
        let owned = owner_qualified_callees(files, caller);
        assert!(!owned.iter().any(|c| c == callee), "{caller}: {owned:?}");
    }
}

#[test]
fn cpp_receiver_typed_as_an_explicit_specialization_binds_its_members_or_declines_a_tie() {
    let calls = b"#include \"../include/slot.h\"\nint read_int() { Slot<int> s; return s.load(); }\nint read_float() { Slot<float> f; return f.load(); }\n";
    let header = b"template <class T> class Slot {\npublic:\n  T load() const { return v_; }\nprivate:\n  T v_;\n};\ntemplate <> class Slot<int> {\npublic:\n  int load() const { return 0; }\n};\n";
    let files: &[(&str, Lang, &[u8])] = &[
        ("src/main.cpp", Lang::Cpp, calls),
        ("include/slot.h", Lang::Cpp, header),
    ];
    let expectations = [
        ("read_int", "Slot<int>.load", true),
        ("read_int", "Slot.load", false),
        ("read_float", "Slot<int>.load", false),
    ];
    for (caller, callee, bound) in expectations {
        let owned = owner_qualified_callees(files, caller);
        assert_eq!(
            owned.iter().any(|c| c == callee),
            bound,
            "{caller}: {owned:?}"
        );
    }
    let tied: &[(&str, Lang, &[u8])] = &[
        ("src/main.cpp", Lang::Cpp, calls),
        ("include/slot.h", Lang::Cpp, header),
        (
            "include/slot_again.h",
            Lang::Cpp,
            b"template <> class Slot<int> {\npublic:\n  int load() const { return 1; }\n};\n",
        ),
    ];
    let owned = owner_qualified_callees(tied, "read_int");
    assert!(!owned.iter().any(|c| c.starts_with("Slot")), "{owned:?}");
}

#[test]
fn cpp_untyped_member_call_declines_rather_than_preferring_the_callers_own_class() {
    let sink = b"class sink {\npublic:\n  void flush();\n  void push_back(int v);\n};\nvoid sink::flush() {}\nvoid sink::push_back(int v) {}\n";
    let pool = b"class pool {\npublic:\n  void flush();\n  void push_back(int v);\n  void drain(auto& q) { q.flush(); }\n  void fill(auto& q) { q.push_back(1); }\n};\nvoid pool::flush() {}\nvoid pool::push_back(int v) {}\n";
    let files: &[(&str, Lang, &[u8])] = &[
        ("src/sink.cpp", Lang::Cpp, sink),
        ("src/pool.cpp", Lang::Cpp, pool),
    ];
    let drained = owner_qualified_callees(files, "drain");
    let filled = owner_qualified_callees(files, "fill");
    assert!(!drained.iter().any(|c| c == "pool.flush"), "{drained:?}");
    assert!(!filled.iter().any(|c| c == "pool.push_back"), "{filled:?}");
}

#[test]
fn cpp_alias_of_a_wrapper_types_its_pointee_and_forward_declarations_do_not_hide_the_class() {
    let files: &[(&str, Lang, &[u8])] = &[
        (
            "src/registry.cpp",
            Lang::Cpp,
            b"#include \"../include/registry.h\"\nvoid Registry::attach(sink_ptr s) { s->flush(); }\nvoid Registry::drop(sink_ptr s) { s.reset(); }\nvoid Registry::tune(logger* l) { l->set_level(1); }\n",
        ),
        (
            "include/sink.h",
            Lang::Cpp,
            b"#pragma once\n#include <memory>\nclass sink {\npublic:\n  void flush();\n  void reset();\n};\nusing sink_ptr = std::shared_ptr<sink>;\n",
        ),
        (
            "include/stale.h",
            Lang::Cpp,
            b"class stale {\npublic:\n  void flush();\n  void reset();\n};\n",
        ),
        ("include/fwd.h", Lang::Cpp, b"class logger;\n"),
        (
            "include/logger.h",
            Lang::Cpp,
            b"#pragma once\nclass logger {\npublic:\n  void set_level(int l);\n};\n",
        ),
        (
            "include/registry.h",
            Lang::Cpp,
            b"#pragma once\n#include \"logger.h\"\nclass Registry {\npublic:\n  void attach(sink_ptr s);\n  void drop(sink_ptr s);\n  void tune(logger* l);\n};\n",
        ),
    ];
    assert!(
        owner_qualified_callees(files, "attach")
            .iter()
            .any(|c| c == "sink.flush"),
        "{:?}",
        owner_qualified_callees(files, "attach")
    );
    let dropped = owner_qualified_callees(files, "drop");
    assert!(!dropped.iter().any(|c| c == "sink.reset"), "{dropped:?}");
    assert!(
        owner_qualified_callees(files, "tune")
            .iter()
            .any(|c| c == "logger.set_level"),
        "{:?}",
        owner_qualified_callees(files, "tune")
    );
}
