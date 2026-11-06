//! C-family type aliases: `typedef T Name;` and C++ `using Name = T;`, as `(Name, declared target)`.

use crate::languages::common::reads::nodes;
use crate::languages::common::typed::declared_type_text;
use tree_sitter::Node;
/// The type a declaration names: `using X = T;` wraps `T` in a `type_descriptor`.
fn target_type(ty: Node<'_>) -> Node<'_> {
    if ty.kind() == "type_descriptor" {
        ty.child_by_field_name("type").unwrap_or(ty)
    } else {
        ty
    }
}

pub(crate) fn type_aliases(root: Node<'_>, source: &[u8]) -> Vec<(String, String)> {
    nodes(root)
        .filter_map(|node| {
            let (name, ty) = match node.kind() {
                "alias_declaration" => (
                    node.child_by_field_name("name")?,
                    node.child_by_field_name("type")?,
                ),
                "type_definition" => (
                    node.child_by_field_name("declarator")?,
                    node.child_by_field_name("type")?,
                ),
                _ => return None,
            };
            if name.kind() != "type_identifier" {
                return None;
            }
            let alias = name.utf8_text(source).ok()?;
            let target = declared_type_text(target_type(ty), source, &[])?;
            (alias != target.trim_end_matches("<>")).then(|| (alias.to_string(), target))
        })
        .collect()
}
