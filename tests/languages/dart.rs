use super::harness::{assert_call_result_binds_factory_return, owner_qualified_callees};
use ripwire_rs::Lang;

const STORES: &str = "class DiskStore {\n  Future<int> openSession() async => 1;\n}\n\
class MemoryStore {\n  Future<int> openSession() async => 2;\n}\n\
class Base {\n  void close() {}\n}\n\
class Pooled extends Base {}\n";

const SERVICE: &str = "class Service {\n  DiskStore? _store;\n\
  void viaField() {\n    _store!.openSession();\n  }\n\
  void viaParam(MemoryStore memory) {\n    memory.openSession();\n  }\n\
  void viaLocal() {\n    final local = DiskStore();\n    local.openSession();\n  }\n\
  void viaInherited(Pooled pooled) {\n    pooled.close();\n  }\n}\n";

fn dart_callees(caller: &str) -> Vec<String> {
    owner_qualified_callees(
        &[
            ("lib/src/stores.dart", Lang::Dart, STORES.as_bytes()),
            ("lib/src/service.dart", Lang::Dart, SERVICE.as_bytes()),
        ],
        caller,
    )
}

#[test]
fn dart_typed_receivers_pick_the_declared_class_method() {
    assert_eq!(dart_callees("viaField"), ["DiskStore.openSession"]);
    assert_eq!(dart_callees("viaParam"), ["MemoryStore.openSession"]);
    assert_eq!(
        dart_callees("viaLocal"),
        [".DiskStore", "DiskStore.openSession"],
        "the constructor call and the method on the constructed type"
    );
}

#[test]
fn dart_typed_receiver_without_the_method_keeps_name_resolution() {
    assert_eq!(dart_callees("viaInherited"), ["Base.close"]);
}

#[test]
fn dart_call_result_receiver_uses_the_factory_return_type() {
    assert_call_result_binds_factory_return(
        Lang::Dart,
        "lib/service.dart",
        "class DiskStore {\n  int open() => 1;\n}\n\
class MemoryStore {\n  int open() => 2;\n}\n\
DiskStore makeStore() => DiskStore();\n\
void run() {\n  makeStore().open();\n}\n",
    );
}

const CONSTRUCTED: &str = "class Point {\n  final int x;\n  Point(this.x);\n\
  Point.origin() : x = 0;\n  factory Point.parse(String s) => Point.origin();\n}\n\
class Box {\n  final Point p;\n  Box(this.p);\n}\n\
void build() {\n  final a = Point(1);\n  final b = Point.origin();\n  final c = Box(a);\n}\n\
void usesParse() {\n  Point.parse('1');\n}\n";

#[test]
fn dart_unnamed_constructor_call_binds_to_the_class_alone() {
    assert_eq!(
        dart_constructed_callees("build"),
        [".Box", ".Point", "Point.origin"]
    );
}

#[test]
fn dart_named_factory_call_binds_by_owner() {
    assert_eq!(dart_constructed_callees("usesParse"), ["Point.parse"]);
}

fn dart_constructed_callees(caller: &str) -> Vec<String> {
    owner_qualified_callees(
        &[("lib/shapes.dart", Lang::Dart, CONSTRUCTED.as_bytes())],
        caller,
    )
}
