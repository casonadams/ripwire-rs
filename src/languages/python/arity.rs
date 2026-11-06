//! Arity of a Python definition as a call sees it. A class is called as its constructor: `__init__`
//! without the receiver, or nothing when the class body defines none.
use crate::gate::contract::{ParamArity, extract_param_arity};
use crate::languages::SyntaxKinds;
use tree_sitter::Node;

pub(super) fn python_def_arity(
    def_node: Node<'_>,
    kinds: &SyntaxKinds,
    source: &[u8],
) -> ParamArity {
    if def_node.kind() != "class_definition" {
        return extract_param_arity(def_node, kinds);
    }
    def_node
        .child_by_field_name("body")
        .and_then(|body| init_definition(body, source))
        .map_or(ParamArity::default(), |init| {
            extract_param_arity(init, kinds).without_receiver()
        })
}

fn init_definition<'a>(body: Node<'a>, source: &[u8]) -> Option<Node<'a>> {
    let mut cursor = body.walk();
    body.named_children(&mut cursor)
        .filter_map(|stmt| match stmt.kind() {
            "decorated_definition" => stmt.child_by_field_name("definition"),
            _ => Some(stmt),
        })
        .find(|def| {
            def.kind() == "function_definition"
                && def
                    .child_by_field_name("name")
                    .and_then(|name| name.utf8_text(source).ok())
                    == Some("__init__")
        })
}
