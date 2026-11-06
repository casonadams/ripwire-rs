//! The class a bare receiver is declared to hold: `c.m()` where `c` is annotated `c: Cache` or
//! assigned `c = Cache(...)` in the same function. Such a call qualifies by `Cache`, so it binds to
//! `Cache.m` and never to a same-named method of another class.

use crate::languages::is_plain_type_name;
use crate::languages::reads::nodes;
use tree_sitter::Node;

/// The declared class of the receiver of the call named by `callee`, when every binding of that
/// receiver name in the enclosing function (or module) agrees on one class.
pub(super) fn declared_receiver_class(callee: Node<'_>, source: &[u8]) -> Option<String> {
    let attribute = callee
        .parent()
        .filter(|p| p.kind() == "attribute" && p.child_by_field_name("attribute") == Some(callee))?;
    attribute
        .parent()
        .filter(|c| c.kind() == "call" && c.child_by_field_name("function") == Some(attribute))?;
    let receiver = attribute
        .child_by_field_name("object")
        .filter(|o| o.kind() == "identifier")?;
    let name = receiver.utf8_text(source).ok()?;
    if matches!(name, "self" | "cls") {
        return None;
    }
    agreed_binding_type(enclosing_body(receiver), name, source)
}

/// The innermost function containing `node`, or the module when there is none.
fn enclosing_body(node: Node<'_>) -> Node<'_> {
    let mut current = node;
    while let Some(parent) = current.parent() {
        if parent.kind() == "function_definition" {
            return parent;
        }
        current = parent;
    }
    current
}

/// The one type every binding of `name` under `scope` declares. A binding without a declared
/// class, or two bindings that disagree, leaves the receiver untyped.
fn agreed_binding_type(scope: Node<'_>, name: &str, source: &[u8]) -> Option<String> {
    let mut agreed: Option<String> = None;
    for node in nodes(scope) {
        let Some(declared) = binding_type(node, name, source) else {
            continue;
        };
        let ty = declared?;
        match &agreed {
            Some(seen) if *seen != ty => return None,
            Some(_) => {}
            None => agreed = Some(ty),
        }
    }
    agreed
}

/// `Some(declared)` when `node` binds `name`, where `declared` is the class it states, if any.
fn binding_type(node: Node<'_>, name: &str, source: &[u8]) -> Option<Option<String>> {
    match node.kind() {
        "typed_parameter" => {
            let param = node.named_child(0)?;
            (param.utf8_text(source).ok()? == name).then(|| annotation(node, source))
        }
        "typed_default_parameter" => {
            let param = node.child_by_field_name("name")?;
            (param.utf8_text(source).ok()? == name).then(|| annotation(node, source))
        }
        "assignment" => {
            let left = node.child_by_field_name("left")?;
            (left.kind() == "identifier" && left.utf8_text(source).ok()? == name).then(|| {
                annotation(node, source).or_else(|| {
                    node.child_by_field_name("right")
                        .and_then(|right| constructed_class(right, source))
                })
            })
        }
        "identifier" => {
            let untyped_binding = node.parent().is_some_and(|p| {
                matches!(p.kind(), "parameters" | "lambda_parameters" | "default_parameter")
                    || (p.kind() == "for_statement"
                        && p.child_by_field_name("left") == Some(node))
            });
            (untyped_binding && node.utf8_text(source).ok()? == name).then_some(None)
        }
        _ => None,
    }
}

/// A plain class name in an annotation (`Cache`), not a generic or a dotted path.
pub(super) fn annotation(node: Node<'_>, source: &[u8]) -> Option<String> {
    type_annotation(node.child_by_field_name("type")?, source)
}

/// The plain class name a type node names (`Cache`), not a generic or a dotted path.
pub(super) fn type_annotation(ty: Node<'_>, source: &[u8]) -> Option<String> {
    let inner = if ty.kind() == "type" && ty.named_child_count() == 1 {
        ty.named_child(0)?
    } else {
        ty
    };
    let text = inner.utf8_text(source).ok()?;
    (inner.kind() == "identifier" && is_plain_type_name(text)).then(|| text.to_string())
}

/// A class by naming convention: a plain, capitalised name.
pub(super) fn is_class_name(text: &str) -> bool {
    text.starts_with(|c: char| c.is_ascii_uppercase()) && is_plain_type_name(text)
}

/// `Cls(...)` constructs a `Cls`: a call of a class name.
pub(super) fn constructed_class(right: Node<'_>, source: &[u8]) -> Option<String> {
    let callee = right
        .child_by_field_name("function")
        .filter(|_| right.kind() == "call" && right.child(0).is_some())?;
    if callee.kind() != "identifier" {
        return None;
    }
    callee
        .utf8_text(source)
        .ok()
        .filter(|text| is_class_name(text))
        .map(str::to_owned)
}
