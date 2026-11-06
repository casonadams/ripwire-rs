use super::harness::{assert_call_result_binds_factory_return, owner_qualified_callees};
use crate::support::callees::callees_described;
use ripwire_rs::Lang;

#[test]
fn java_call_result_receiver_uses_the_factory_return_type() {
    assert_call_result_binds_factory_return(
        Lang::Java,
        "src/Service.java",
        "class DiskStore { int open() { return 1; } }\n\
class MemoryStore { int open() { return 2; } }\n\
class Service {\n  DiskStore make() { return new DiskStore(); }\n  void run() { make().open(); }\n}\n",
    );
}

const ELEMENT: &str = "package nodes;\n\
public class Element {\n  public Elements select(String q) { return null; }\n\
  public Element body() { return null; }\n  public Element get() { return this; }\n}\n";

const DOCUMENT: &str = "package nodes;\npublic class Document extends Element {\n}\n";

const ELEMENTS: &str = "package select;\nimport nodes.Element;\n\
public class Elements {\n  public Element first() { return null; }\n}\n";

const CLIENT: &str = "package app;\nimport java.util.List;\n\
import nodes.Document;\nimport nodes.Element;\n\
public class Client {\n\
  void viaChain(Document doc) { doc.select(\"img\").first(); }\n\
  void viaInherited(Document doc) { doc.body(); }\n\
  void viaCast(Object o) { ((Element) o).body(); }\n\
  void viaLoop(Document doc) { for (Element link : doc.select(\"a\")) { link.body(); } }\n\
  void viaBuiltin(List<String> names) { names.get(0); }\n\
  void viaInTree(Element e) { e.get(); }\n}\n";

#[test]
fn java_chains_inheritance_casts_loops_and_std_names_type_across_files() {
    let files: &[(&str, Lang, &[u8])] = &[
        ("src/nodes/Element.java", Lang::Java, ELEMENT.as_bytes()),
        ("src/nodes/Document.java", Lang::Java, DOCUMENT.as_bytes()),
        ("src/select/Elements.java", Lang::Java, ELEMENTS.as_bytes()),
        ("src/app/Client.java", Lang::Java, CLIENT.as_bytes()),
    ];
    assert_eq!(
        owner_qualified_callees(files, "viaChain"),
        ["Element.select", "Elements.first"]
    );
    assert_eq!(
        owner_qualified_callees(files, "viaInherited"),
        ["Element.body"]
    );
    assert_eq!(owner_qualified_callees(files, "viaCast"), ["Element.body"]);
    assert_eq!(
        owner_qualified_callees(files, "viaLoop"),
        ["Element.body", "Element.select"]
    );
    assert!(
        !owner_qualified_callees(files, "viaBuiltin").contains(&"Element.get".to_string()),
        "a List.get call must not bind to the in-tree Element.get"
    );
    assert_eq!(owner_qualified_callees(files, "viaInTree"), ["Element.get"]);
}

#[test]
fn java_import_names_the_class_its_package_declares() {
    let parser_tag =
        "package a.parser;\npublic class Tag {\n  public String namespace() { return \"x\"; }\n}\n";
    let token_tag =
        "package a.token;\npublic class Tag {\n  public String namespace() { return \"y\"; }\n}\n";
    let element = "package a.nodes;\nimport a.parser.Tag;\n\
public class Element {\n  Tag tag;\n  String ns() { return tag.namespace(); }\n}\n";
    let files: &[(&str, Lang, &[u8])] = &[
        ("src/a/parser/Tag.java", Lang::Java, parser_tag.as_bytes()),
        ("src/a/token/Tag.java", Lang::Java, token_tag.as_bytes()),
        ("src/a/nodes/Element.java", Lang::Java, element.as_bytes()),
    ];
    let callees = callees_described(files, "ns", |ingest, interner, id| {
        let sym = &ingest.symbols[id as usize];
        format!(
            "{}:{}",
            ingest.file_path(sym.file_id, interner),
            interner.resolve(&sym.name)
        )
    });
    assert_eq!(callees, ["src/a/parser/Tag.java:namespace"]);
}
