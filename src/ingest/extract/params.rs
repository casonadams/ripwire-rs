const NON_BINDING_PARAM_FIELDS: &[&str] = &[
    "value",
    "right",
    "type",
    "default_type",
    "bounds",
    "constraint",
    "decorator",
    "type_arguments",
];

/// True when a function or closure enclosing `node` declares a parameter named `name`. `ancestors` is the
/// chain of `node`'s ancestors, nearest first.
pub(super) fn is_enclosing_parameter(
    ancestors: &[tree_sitter::Node<'_>],
    node: tree_sitter::Node<'_>,
    name: &str,
    source_bytes: &[u8],
) -> bool {
    ancestors.iter().any(|n| {
        n.child_by_field_name("parameters").is_some_and(|params| {
            node.start_byte() >= params.end_byte() && binds_name(params, name, source_bytes)
        })
    })
}

fn binds_name(node: tree_sitter::Node<'_>, name: &str, source_bytes: &[u8]) -> bool {
    if node.child_count() == 0 {
        return node.utf8_text(source_bytes).is_ok_and(|t| t == name);
    }
    (0..node.child_count()).any(|i| {
        node.field_name_for_child(i as u32)
            .is_none_or(|f| !NON_BINDING_PARAM_FIELDS.contains(&f))
            && node
                .child(i)
                .is_some_and(|c| binds_name(c, name, source_bytes))
    })
}
