use super::binding_types::{binding_type, self_binding_type};
use super::scope::{field_text, rust_binding_range_in, rust_scope};
use super::types::{TypeCx, extract_type_repr};

fn is_self_binding(node: tree_sitter::Node<'_>) -> bool {
    node.kind() == "self_parameter"
        || (node.kind() == "self" && node.parent().is_some_and(|p| p.kind() == "parameter"))
}

/// The name of a `const` or `static` item. The item is the definition that reads of the name reach, not a
/// local binding that shadows them, so reads inside its scope stay references to the item.
fn is_item_name(node: tree_sitter::Node<'_>, ancestors: &[tree_sitter::Node<'_>]) -> bool {
    ancestors.last().is_some_and(|parent| {
        matches!(parent.kind(), "const_item" | "static_item")
            && super::super::reads::in_field(*parent, node, "name")
    })
}

/// Declared field types of every `struct`, as `(struct, field)` members, so `x.field.method()` types its receiver.
fn record_struct_fields(
    cx: &TypeCx<'_>,
    root: tree_sitter::Node<'_>,
    bindings: &mut super::super::reads::ReadBindings<'_>,
) {
    for item in super::super::reads::nodes(root).filter(|n| n.kind() == "struct_item") {
        let (Some(owner), Some(body)) = (
            field_text(item, "name", cx.source),
            item.child_by_field_name("body"),
        ) else {
            continue;
        };
        let mut cursor = body.walk();
        for field in body
            .children(&mut cursor)
            .filter(|c| c.kind() == "field_declaration")
        {
            if let (Some(name), Some(ty)) = (
                field_text(field, "name", cx.source),
                field
                    .child_by_field_name("type")
                    .and_then(|ty| extract_type_repr(cx, ty)),
            ) {
                bindings.insert_member(owner, name, ty);
            }
        }
    }
}

/// Return types of free functions (not methods), keyed by name; two functions of one name that disagree
/// leave the name untyped.
fn record_free_returns<'a>(
    cx: &TypeCx<'a>,
    root: tree_sitter::Node<'_>,
    bindings: &mut super::super::reads::ReadBindings<'a>,
) {
    let mut returns = super::super::typed::AgreedReturns::default();
    for item in super::super::reads::nodes(root).filter(|n| n.kind() == "function_item") {
        let in_impl = item
            .parent()
            .and_then(|body| body.parent())
            .is_some_and(|owner| matches!(owner.kind(), "impl_item" | "trait_item"));
        let Some(name) = field_text(item, "name", cx.source).filter(|_| !in_impl) else {
            continue;
        };
        let ty = item
            .child_by_field_name("return_type")
            .and_then(|ty| extract_type_repr(cx, ty));
        returns.note(name, ty);
    }
    returns.insert_into(bindings);
}

pub(super) fn collect_rust_read_bindings<'a>(
    root: tree_sitter::Node<'_>,
    source: &'a [u8],
) -> super::super::reads::ReadBindings<'a> {
    let cx = TypeCx::new(root, source);
    let mut bindings = super::super::reads::ReadBindings::default();
    record_struct_fields(&cx, root, &mut bindings);
    record_free_returns(&cx, root, &mut bindings);
    super::super::reads::walk_with_ancestors(root, |node, ancestors| {
        if is_self_binding(node) {
            if let Some(func) = super::super::reads::enclosing_scope(node, rust_scope) {
                bindings.insert("self", func.byte_range(), self_binding_type(&cx, node));
            }
        } else if let Some(range) = rust_binding_range_in(node, ancestors)
            && !is_item_name(node, ancestors)
            && let Ok(name) = node.utf8_text(source)
        {
            bindings.insert(name, range, binding_type(&cx, node));
        }
    });
    bindings
}
