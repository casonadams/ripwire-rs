use super::harness::{GraphProtocolCase, owner_qualified_callees, sym, verify_protocol_case};
use ripwire_rs::{Lang, SymKind};

const JAVA_STORES: &str = "class DiskStore { int open() { return 1; } }\n\
class MemoryStore { int open() { return 2; } }\n\
class Base { void close() {} }\n\
class Pooled extends Base {}\n";

const JAVA_SERVICE: &str = "class Service {\n  private DiskStore store;\n\
  void viaField() { store.open(); }\n\
  void viaThisField() { this.store.open(); }\n\
  void viaParam(MemoryStore memory) { memory.open(); }\n\
  void viaLocal() { var local = new DiskStore(); local.open(); }\n\
  void viaInherited(Pooled pooled) { pooled.close(); }\n}\n";

fn java_callees(caller: &str) -> Vec<String> {
    owner_qualified_callees(
        &[
            ("src/Stores.java", Lang::Java, JAVA_STORES.as_bytes()),
            ("src/Service.java", Lang::Java, JAVA_SERVICE.as_bytes()),
        ],
        caller,
    )
}

#[test]
fn java_typed_receivers_pick_the_declared_class_method() {
    assert_eq!(java_callees("viaField"), ["DiskStore.open"]);
    assert_eq!(java_callees("viaThisField"), ["DiskStore.open"]);
    assert_eq!(java_callees("viaParam"), ["MemoryStore.open"]);
    assert!(java_callees("viaLocal").contains(&"DiskStore.open".to_string()));
    assert!(!java_callees("viaLocal").contains(&"MemoryStore.open".to_string()));
}

#[test]
fn java_typed_receiver_without_the_method_keeps_name_resolution() {
    assert_eq!(java_callees("viaInherited"), ["Base.close"]);
}

#[test]
fn java_subclass_calls_reach_the_inherited_base_method_over_same_named_methods_elsewhere() {
    let classes = "class Base { void close() {} void helper() {} }\n\
class Other { void close() {} void helper() {} }\n\
class Pooled extends Base { void work() { helper(); } }\n\
class User { void run(Pooled pooled) { pooled.close(); } }\n";
    let callees = |caller| {
        owner_qualified_callees(&[("src/Pool.java", Lang::Java, classes.as_bytes())], caller)
    };
    assert_eq!(callees("run"), ["Base.close"]);
    assert_eq!(callees("work"), ["Base.helper"]);
}

#[test]
fn java_call_result_receiver_uses_the_factory_return_type() {
    let classes = "class DiskStore { int open() { return 1; } }\n\
class MemoryStore { int open() { return 2; } }\n\
class Service {\n  DiskStore make() { return new DiskStore(); }\n  void run() { make().open(); }\n}\n";
    let callees = owner_qualified_callees(
        &[("src/Service.java", Lang::Java, classes.as_bytes())],
        "run",
    );
    assert!(
        callees.contains(&"DiskStore.open".to_string()),
        "{callees:?}"
    );
    assert!(
        !callees.contains(&"MemoryStore.open".to_string()),
        "{callees:?}"
    );
}

#[test]
fn java_call_prefers_the_method_taking_exactly_its_argument_count() {
    let classes = "class Single { void stash(int a) {} }\n\
class Pair { void stash(int a, int b) {} }\n\
class Client {\n  void one(Object store) { store.stash(1); }\n  void two(Object store) { store.stash(1, 2); }\n}\n";
    let callees = |caller| {
        owner_qualified_callees(
            &[("src/Client.java", Lang::Java, classes.as_bytes())],
            caller,
        )
    };
    assert_eq!(callees("one"), ["Single.stash"]);
    assert_eq!(callees("two"), ["Pair.stash"]);
}

#[test]
fn java_interface_typed_call_keeps_the_interface_declaration() {
    let classes = "interface Store { void stash(int a); }\n\
class Disk implements Store { public void stash(int a) {} }\n\
class Client {\n  void run(Store store) { store.stash(1); }\n}\n";
    let callees = owner_qualified_callees(
        &[("src/Client.java", Lang::Java, classes.as_bytes())],
        "run",
    );
    assert!(callees.contains(&"Store.stash".to_string()), "{callees:?}");
}

#[test]
fn java_record_and_annotation_declarations_are_definitions() {
    verify_protocol_case(&GraphProtocolCase {
        label: "java-record",
        files: &[(
            "src/Points.java",
            Lang::Java,
            b"public record Point(int x, int y) {\n    public int sum() { return x + y; }\n}\n@interface Marker { String value(); }\n",
        )],
        expected_syms: &[
            sym("Point", SymKind::Class, None, true),
            sym("sum", SymKind::Method, Some("Point"), true),
            sym("Marker", SymKind::Interface, None, true),
        ],
        ..Default::default()
    });
}
