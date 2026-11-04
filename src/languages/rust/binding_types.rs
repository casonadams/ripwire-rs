use super::scope::{field_text, rust_scope};
use crate::languages::reads::enclosing_scope;
use super::types::{
    TypeCx, bare_path, enclosing_self_type, extract_type_repr, names_std_wrapper, self_or_name,
};

fn first_call_argument(call: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let args = call.child_by_field_name("arguments")?;
    let mut cursor = args.walk();
    args.children(&mut cursor)
        .find(|arg| !matches!(arg.kind(), "(" | ")" | ","))
}

fn associated_call_type(cx: &TypeCx<'_>, subject: &str, name: &str) -> Option<String> {
    match cx.assoc_returns.get(subject).and_then(|fns| fns.get(name)) {
        Some(ret) => ret.clone(),
        None if matches!(name, "new" | "default") => Some(subject.to_string()),
        None => None,
    }
}

fn extract_call_type(
    cx: &TypeCx<'_>,
    func: tree_sitter::Node<'_>,
    call: tree_sitter::Node<'_>,
) -> Option<String> {
    if func.kind() != "scoped_identifier" {
        return None;
    }
    let path = bare_path(field_text(func, "path", cx.source)?);
    let name = field_text(func, "name", cx.source)?;
    if name == "new" && names_std_wrapper(cx, func, path) {
        return first_call_argument(call).and_then(|arg| extract_let_value_type(cx, arg));
    }
    let subject = self_or_name(cx, func, path)?;
    associated_call_type(cx, &subject, name)
}

fn extract_let_value_type(cx: &TypeCx<'_>, value: tree_sitter::Node<'_>) -> Option<String> {
    match value.kind() {
        "call_expression" => extract_call_type(cx, value.child_by_field_name("function")?, value),
        "struct_expression" => {
            let name = field_text(value, "name", cx.source)?;
            self_or_name(cx, value, name).map(std::borrow::Cow::into_owned)
        }
        "identifier" => {
            let name = value.utf8_text(cx.source).ok()?;
            cx.unit_structs.contains(name).then(|| name.to_string())
        }
        _ => None,
    }
}

fn extract_let_type(cx: &TypeCx<'_>, let_decl: tree_sitter::Node<'_>) -> Option<String> {
    match let_decl.child_by_field_name("type") {
        Some(ty) => extract_type_repr(cx, ty),
        None => extract_let_value_type(cx, let_decl.child_by_field_name("value")?),
    }
}

fn extract_param_type(cx: &TypeCx<'_>, param: tree_sitter::Node<'_>) -> Option<String> {
    extract_type_repr(cx, param.child_by_field_name("type")?)
}

pub(super) fn self_binding_type(cx: &TypeCx<'_>, node: tree_sitter::Node<'_>) -> Option<String> {
    enclosing_of_kind(node, "parameter")
        .and_then(|param| extract_param_type(cx, param))
        .or_else(|| enclosing_self_type(cx, node))
}

/// A pattern whose only wrappers are `mut`: the bound identifier is the whole matched value.
fn is_direct_binding(node: tree_sitter::Node<'_>, pattern: tree_sitter::Node<'_>) -> bool {
    let mut cur = node;
    while cur != pattern {
        match cur.parent() {
            Some(parent) if parent.kind() == "mut_pattern" => cur = parent,
            _ => return false,
        }
    }
    true
}

pub(super) fn binding_type(cx: &TypeCx<'_>, node: tree_sitter::Node<'_>) -> Option<String> {
    if let Some(let_decl) = enclosing_of_kind(node, "let_declaration") {
        let pattern = let_decl.child_by_field_name("pattern")?;
        return if is_direct_binding(node, pattern) {
            extract_let_type(cx, let_decl)
        } else {
            None
        };
    }
    let param = enclosing_of_kind(node, "parameter")?;
    let pattern = param.child_by_field_name("pattern")?;
    if is_direct_binding(node, pattern) {
        extract_param_type(cx, param)
    } else {
        None
    }
}

fn enclosing_of_kind<'a>(node: tree_sitter::Node<'a>, kind: &str) -> Option<tree_sitter::Node<'a>> {
    enclosing_scope(node, |k| k == kind || rust_scope(k)).filter(|n| n.kind() == kind)
}
