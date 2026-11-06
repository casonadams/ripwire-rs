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

#[test]
fn dart_named_factory_binds_by_class_through_a_package_import() {
    assert_eq!(package_factory_callees("parseInput"), ["XmlDocument.parse"]);
    assert_eq!(package_factory_callees("numbers"), ["XPathInteger.fromInt"]);
}

#[test]
fn dart_factory_named_like_a_std_member_binds_to_its_class_only() {
    assert_eq!(package_factory_callees("tokenize"), ["Tokens.split"]);
    assert!(
        package_factory_callees("literal").is_empty(),
        "a string literal's split is the builtin, not the in-tree factory"
    );
}

const XML_LIBRARY: &str = "library xml;\nexport 'src/xml/nodes/document.dart' show XmlDocument;\n\
export 'src/xpath/numeric.dart' show XPathInteger;\n";
const DOCUMENT: &str = "class XmlDocument {\n  XmlDocument();\n\
  factory parse(String input) => XmlDocument();\n}\n";
const NUMERIC: &str = "class XPathInteger {\n  factory fromInt(int val) => XPathInteger();\n}\n";
const TOKENS: &str = "class Tokens {\n  Tokens();\n  factory split(String text) => Tokens();\n}\n";
const CALLERS: &str = "import 'package:xml/xml.dart';\n\
import 'package:xml/src/xpath/numeric.dart';\nimport 'package:xml/src/tokens.dart';\n\
void parseInput() {\n  XmlDocument.parse('x');\n}\n\
void numbers() {\n  XPathInteger.fromInt(1);\n}\n\
void tokenize() {\n  Tokens.split('a,b');\n}\n\
void literal() {\n  'a,b'.split(',');\n}\n\
void viaInstance(dynamic other) {\n  other.parse('x');\n}\n";

fn package_factory_callees(caller: &str) -> Vec<String> {
    owner_qualified_callees(
        &[
            ("lib/xml.dart", Lang::Dart, XML_LIBRARY.as_bytes()),
            (
                "lib/src/xml/nodes/document.dart",
                Lang::Dart,
                DOCUMENT.as_bytes(),
            ),
            ("lib/src/xpath/numeric.dart", Lang::Dart, NUMERIC.as_bytes()),
            ("lib/src/tokens.dart", Lang::Dart, TOKENS.as_bytes()),
            ("lib/src/use.dart", Lang::Dart, CALLERS.as_bytes()),
        ],
        caller,
    )
}

#[test]
fn dart_factory_is_not_reached_as_an_instance_member() {
    assert!(
        package_factory_callees("viaInstance").is_empty(),
        "a factory constructor is reached only through its class name"
    );
}

const HOLDERS: &str = "class Kids {\n  void add(int v) {}\n}\n\
class Holder<T> {\n  void add(T v) {}\n}\n\
class Doc {\n  Kids get kids => Kids();\n}\n";
const HOLDER_CALLERS: &str = "Holder<Kids> holderOf() => Holder<Kids>();\n\
void viaGetter(Doc d) {\n  d.kids.add(1);\n}\n\
void viaGenericReturn() {\n  holderOf().add(Kids());\n}\n";

const HOLDER_FILES: &[(&str, Lang, &[u8])] = &[
    ("lib/holders.dart", Lang::Dart, HOLDERS.as_bytes()),
    ("lib/use.dart", Lang::Dart, HOLDER_CALLERS.as_bytes()),
];

fn holder_callees(caller: &str) -> Vec<String> {
    owner_qualified_callees(HOLDER_FILES, caller)
}

#[test]
fn dart_getter_return_types_the_next_receiver() {
    assert_eq!(holder_callees("viaGetter"), ["Kids.add"]);
}

#[test]
fn dart_generic_return_types_by_its_in_tree_base_class() {
    assert_eq!(
        holder_callees("viaGenericReturn"),
        [".Kids", ".holderOf", "Holder.add"]
    );
}

const METHOD_CHAIN_DOC: &str = "class Kids {\n  void add(int v) {}\n}\n\
class Doc {\n  Kids holder() => Kids();\n  Doc self() => this;\n}\n";
const METHOD_CHAIN_CALLERS: &str = "void viaMethod(Doc d) {\n  d.holder().add(1);\n}\n\
void viaChainedMethods(Doc d) {\n  d.self().holder().add(2);\n}\n";

const METHOD_CHAIN_FILES: &[(&str, Lang, &[u8])] = &[
    ("lib/doc.dart", Lang::Dart, METHOD_CHAIN_DOC.as_bytes()),
    ("lib/use.dart", Lang::Dart, METHOD_CHAIN_CALLERS.as_bytes()),
];

fn method_chain_callees(caller: &str) -> Vec<String> {
    owner_qualified_callees(METHOD_CHAIN_FILES, caller)
}

#[test]
fn dart_method_return_types_the_next_receiver() {
    assert_eq!(
        method_chain_callees("viaMethod"),
        ["Doc.holder", "Kids.add"]
    );
}

#[test]
fn dart_method_chain_types_each_step_from_its_return() {
    assert_eq!(
        method_chain_callees("viaChainedMethods"),
        ["Doc.holder", "Doc.self", "Kids.add"]
    );
}

const ELEMENT_DOC: &str = "class Kids {\n  void add(int v) {}\n}\n\
class Other {\n  void add(int v) {}\n}\n";
const ELEMENT_CALLERS: &str = "void viaParameter(List<Kids> items) {\n  items.forEach((k) => k.add(1));\n}\n\
void viaLocal() {\n  final List<Kids> local = [];\n  local.map((k) => k.add(2)).toList();\n}\n\
void viaUntypedElement(List<Other> others) {\n  others.where((o) => o.add(3) == null).toList();\n}\n";

const ELEMENT_FILES: &[(&str, Lang, &[u8])] = &[
    ("lib/kids.dart", Lang::Dart, ELEMENT_DOC.as_bytes()),
    ("lib/use.dart", Lang::Dart, ELEMENT_CALLERS.as_bytes()),
];

fn element_callees(caller: &str) -> Vec<String> {
    owner_qualified_callees(ELEMENT_FILES, caller)
}

#[test]
fn dart_lambda_parameter_takes_the_element_of_its_collection() {
    assert_eq!(element_callees("viaParameter"), ["Kids.add"]);
}

#[test]
fn dart_lambda_over_a_local_collection_takes_its_element() {
    assert_eq!(element_callees("viaLocal"), ["Kids.add"]);
}

#[test]
fn dart_lambda_parameter_binds_to_the_element_class_only() {
    assert_eq!(element_callees("viaUntypedElement"), ["Other.add"]);
}

const CONTRACT_DOC: &str = "abstract class Shape {\n  double area();\n}\n\
class Square implements Shape {\n  double area() => 1.0;\n}\n";
const CONTRACT_CALLERS: &str = "double viaDynamic(dynamic x) {\n  return x.area();\n}\n\
double viaElements(List<Shape> shapes) {\n  return shapes.first.area();\n}\n";

const CONTRACT_FILES: &[(&str, Lang, &[u8])] = &[
    ("lib/shape.dart", Lang::Dart, CONTRACT_DOC.as_bytes()),
    ("lib/use.dart", Lang::Dart, CONTRACT_CALLERS.as_bytes()),
];

fn contract_callees(caller: &str) -> Vec<String> {
    owner_qualified_callees(CONTRACT_FILES, caller)
}

#[test]
fn dart_untyped_call_binds_to_the_abstract_declaration_and_its_implementors() {
    assert_eq!(
        contract_callees("viaDynamic"),
        ["Shape.area", "Square.area"]
    );
}

#[test]
fn dart_typed_call_on_an_abstract_class_keeps_the_declaration_and_its_implementors() {
    assert_eq!(
        contract_callees("viaElements"),
        ["Shape.area", "Square.area"]
    );
}

const CORE_DOC: &str = "class Box {\n  bool isA(String t) => true;\n  int abs() => 0;\n}\n";
const CORE_CALLERS: &str = "void viaUntyped(dynamic x) {\n  x.isA('y');\n  x.abs();\n}\n\
void viaTyped(Box b) {\n  b.isA('y');\n}\n";

const CORE_FILES: &[(&str, Lang, &[u8])] = &[
    ("lib/box.dart", Lang::Dart, CORE_DOC.as_bytes()),
    ("lib/use.dart", Lang::Dart, CORE_CALLERS.as_bytes()),
];

fn core_callees(caller: &str) -> Vec<String> {
    owner_qualified_callees(CORE_FILES, caller)
}

#[test]
fn dart_core_member_on_an_untyped_receiver_is_not_bound_by_name() {
    assert_eq!(core_callees("viaUntyped"), Vec::<String>::new());
}

#[test]
fn dart_core_member_on_a_typed_receiver_still_binds_to_its_class() {
    assert_eq!(core_callees("viaTyped"), ["Box.isA"]);
}

const CHECKS: &str = "class Seq {}\nSubject<T> check<T>(T value) => Subject<T>();\nclass Subject<T> {}\n\
Condition<Seq> isSeq(Object? expected) =>\n    (Subject<Seq> subject) => subject.isSeq(expected);\n\
extension SeqCheck on Subject<Seq> {\n  void isSeq(Object? expected) {}\n}\n";

const CHECK_CALLER: &str = "import '../lib/checks.dart';\nvoid viaCheck(Seq result) {\n  check(result).isSeq([3]);\n}\n\
void viaBare(Seq result) {\n  isSeq([3]);\n}\n";

const MIXIN_FILE: &str = "mixin Writer {\n  String toXmlString({bool pretty = false}) => 'x';\n}\n\
class Direct with Writer {}\n\
class Base {}\nabstract class Holder extends Base with Writer {}\n\
class Leaf extends Holder {}\n";

const MIXIN_CALLER: &str = "void viaDirect(Direct d) {\n  d.toXmlString(pretty: true);\n}\n\
void viaLeaf(Leaf l) {\n  l.toXmlString(pretty: true);\n}\n";

const RESOLVE_FILES: &[(&str, Lang, &[u8])] = &[
    ("lib/checks.dart", Lang::Dart, CHECKS.as_bytes()),
    ("test/check_test.dart", Lang::Dart, CHECK_CALLER.as_bytes()),
    ("lib/mixins.dart", Lang::Dart, MIXIN_FILE.as_bytes()),
    ("lib/use.dart", Lang::Dart, MIXIN_CALLER.as_bytes()),
];

#[test]
fn dart_member_calls_bind_to_the_member_they_name() {
    let callees = |caller| owner_qualified_callees(RESOLVE_FILES, caller);
    assert_eq!(callees("viaCheck"), [".check", "SeqCheck.isSeq"]);
    assert_eq!(callees("viaBare"), [".isSeq"]);
    assert_eq!(callees("viaDirect"), ["Writer.toXmlString"]);
}
