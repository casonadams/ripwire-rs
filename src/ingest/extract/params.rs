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

/// True when a function or closure enclosing `node` declares a parameter named `name`.
pub(super) fn is_enclosing_parameter(
    node: tree_sitter::Node<'_>,
    name: &str,
    source_bytes: &[u8],
) -> bool {
    let mut cur = node.parent();
    while let Some(n) = cur {
        if let Some(params) = n.child_by_field_name("parameters")
            && node.start_byte() >= params.end_byte()
            && binds_name(params, name, source_bytes)
        {
            return true;
        }
        cur = n.parent();
    }
    false
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
