use super::binding_types::{binding_type, self_binding_type};
use super::scope::{field_text, rust_binding_range_in, rust_scope};
use super::types::{TypeCx, extract_type_repr, nominal_name};

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

/// Declared member types: struct fields as `(struct, field)`, and inherent method returns as `(type, "name()")`,
/// so `x.field.method()` and `self.name().member` type their receivers.
fn record_members(cx: &TypeCx<'_>, root: tree_sitter::Node<'_>, bindings: &mut super::super::reads::ReadBindings<'_>) {
    for item in super::super::reads::nodes(root) {
        let owner = match item.kind() {
            "struct_item" => field_text(item, "name", cx.source).map(|name| (name, false)),
            "impl_item" if item.child_by_field_name("trait").is_none() => item
                .child_by_field_name("type")
                .and_then(|ty| nominal_name(cx.source, ty))
                .map(|name| (name, true)),
            _ => None,
        };
        let (Some((owner, is_impl)), Some(body)) = (owner, item.child_by_field_name("body")) else {
            continue;
        };
        let mut cursor = body.walk();
        for member in body.children(&mut cursor) {
            let entry = if is_impl {
                method_entry(cx, member)
            } else {
                field_entry(cx, member)
            };
            if let Some((name, ty)) = entry {
                bindings.insert_member(owner, &name, ty);
            }
        }
    }
}

fn field_entry(cx: &TypeCx<'_>, field: tree_sitter::Node<'_>) -> Option<(String, String)> {
    if field.kind() != "field_declaration" {
        return None;
    }
    let name = field_text(field, "name", cx.source)?;
    let ty = field.child_by_field_name("type").and_then(|ty| extract_type_repr(cx, ty))?;
    Some((name.to_string(), ty))
}

fn method_entry(cx: &TypeCx<'_>, item: tree_sitter::Node<'_>) -> Option<(String, String)> {
    if item.kind() != "function_item" {
        return None;
    }
    let name = field_text(item, "name", cx.source)?;
    let ty = item
        .child_by_field_name("return_type")
        .and_then(|ty| extract_type_repr(cx, ty))?;
    Some((format!("{name}()"), ty))
}

/// Return types of free functions (not methods), keyed by name; a name whose functions disagree is untyped.
fn record_free_returns<'a>(cx: &TypeCx<'a>, bindings: &mut super::super::reads::ReadBindings<'a>) {
    for (name, returned) in &cx.free_returns {
        if let Some(ty) = returned.as_ref().and_then(|r| r.ty.clone()) {
            bindings.insert_return(name, ty);
        }
    }
}

pub(super) fn collect_rust_read_bindings<'a>(
    root: tree_sitter::Node<'_>,
    source: &'a [u8],
) -> super::super::reads::ReadBindings<'a> {
    let cx = TypeCx::new(root, source);
    let mut bindings = super::super::reads::ReadBindings::default();
    record_members(&cx, root, &mut bindings);
    record_free_returns(&cx, &mut bindings);
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
