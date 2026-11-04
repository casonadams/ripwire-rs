pub(super) fn javascript_scope(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "function_expression"
            | "generator_function"
            | "generator_function_declaration"
            | "arrow_function"
            | "method_definition"
            | "statement_block"
            | "catch_clause"
            | "for_statement"
            | "for_in_statement"
    )
}

pub(super) fn javascript_binding_target(parent: tree_sitter::Node<'_>, node: tree_sitter::Node<'_>) -> bool {
    javascript_binding_target_of(parent, node, node.parent() == Some(parent))
}

/// [`javascript_binding_target`] with `direct` (is `parent` the parent of `node`) supplied by the caller.
fn javascript_binding_target_of(
    parent: tree_sitter::Node<'_>,
    node: tree_sitter::Node<'_>,
    direct: bool,
) -> bool {
    let field = match parent.kind() {
        "variable_declarator"
        | "function_declaration"
        | "function_expression"
        | "class_declaration"
        | "class"
        | "generator_function"
        | "generator_function_declaration" => "name",
        "required_parameter" | "optional_parameter" => "pattern",
        "catch_clause" | "arrow_function" => "parameter",
        "assignment_pattern" | "pair_pattern" | "for_in_statement" => "left",
        "formal_parameters" => return direct,
        "rest_pattern" | "object_pattern" | "array_pattern" | "import_specifier"
        | "namespace_import" | "import_clause" => return true,
        _ => return false,
    };
    crate::languages::reads::in_field(parent, node, field)
}

pub(crate) fn javascript_binding_scope(
    node: tree_sitter::Node<'_>,
) -> Option<tree_sitter::Node<'_>> {
    let mut ancestors = Vec::new();
    let mut ancestor = node.parent();
    while let Some(parent) = ancestor {
        ancestors.push(parent);
        ancestor = parent.parent();
    }
    ancestors.reverse();
    javascript_binding_scope_in(node, &ancestors)
}

/// The scope `node` is bound in, given its ancestors from the root down to its parent.
pub(crate) fn javascript_binding_scope_in<'a>(
    node: tree_sitter::Node<'a>,
    ancestors: &[tree_sitter::Node<'a>],
) -> Option<tree_sitter::Node<'a>> {
    if !matches!(
        node.kind(),
        "identifier" | "shorthand_property_identifier_pattern"
    ) {
        return None;
    }
    for (up, (depth, &parent)) in ancestors.iter().enumerate().rev().enumerate() {
        if javascript_binding_target_of(parent, node, up == 0) {
            let above = if matches!(
                parent.kind(),
                "function_declaration" | "generator_function_declaration" | "class_declaration"
            ) {
                &ancestors[..depth]
            } else {
                ancestors
            };
            return javascript_binding_owner(parent, &ancestors[..depth], above);
        }
        if javascript_scope(parent.kind()) {
            break;
        }
    }
    None
}

/// The scope that owns `binding`. `binding_above` are the ancestors of `binding`, `owner_above` those of the node
/// the climb starts from.
fn javascript_binding_owner<'a>(
    binding: tree_sitter::Node<'a>,
    binding_above: &[tree_sitter::Node<'a>],
    owner_above: &[tree_sitter::Node<'a>],
) -> Option<tree_sitter::Node<'a>> {
    let function_scoped = binding.kind() == "variable_declarator"
        && binding_above
            .last()
            .is_some_and(|parent| parent.kind() == "variable_declaration");
    owner_above.iter().rev().copied().find(|scope| {
        let kind = scope.kind();
        javascript_scope(kind)
            && (!function_scoped
                || !matches!(
                    kind,
                    "statement_block" | "catch_clause" | "for_statement" | "for_in_statement"
                ))
    })
}
