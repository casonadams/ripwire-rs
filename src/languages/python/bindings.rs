use crate::languages::reads::{NonValueRules, non_value};

fn python_scope(kind: &str) -> bool {
    matches!(
        kind,
        "function_definition"
            | "lambda"
            | "class_definition"
            | "list_comprehension"
            | "set_comprehension"
            | "dictionary_comprehension"
            | "generator_expression"
    )
}

fn python_binding_target(parent: tree_sitter::Node<'_>, node: tree_sitter::Node<'_>) -> bool {
    let field = match parent.kind() {
        "assignment" | "augmented_assignment" | "for_statement" | "for_in_clause" => "left",
        "named_expression"
        | "function_definition"
        | "class_definition"
        | "default_parameter"
        | "typed_default_parameter" => "name",
        "aliased_import" => "alias",
        "parameters" | "lambda_parameters" => return node.parent() == Some(parent),
        "typed_parameter" => return parent.named_child(0) == Some(node),
        "import_statement" => return true,
        "import_from_statement" => {
            return !crate::languages::reads::in_field(parent, node, "module_name");
        }
        "list_splat_pattern" | "dictionary_splat_pattern" | "as_pattern_target" => return true,
        _ => return false,
    };
    crate::languages::reads::in_field(parent, node, field)
}

pub(super) fn python_binding_scope(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    if node.kind() != "identifier" {
        return None;
    }
    let mut ancestor = node.parent();
    while let Some(parent) = ancestor {
        if python_binding_target(parent, node) {
            let owner = if matches!(parent.kind(), "function_definition" | "class_definition") {
                parent
            } else {
                node
            };
            return crate::languages::reads::enclosing_scope(owner, python_scope)
                .filter(|scope| scope.kind() != "class_definition");
        }
        if python_scope(parent.kind()) {
            break;
        }
        ancestor = parent.parent();
    }
    None
}

/// The byte range a binding of `node` is in scope over: its scope's body.
pub(super) fn python_binding_range(node: tree_sitter::Node<'_>) -> Option<std::ops::Range<usize>> {
    python_binding_scope(node).map(|scope| {
        scope
            .child_by_field_name("body")
            .unwrap_or(scope)
            .byte_range()
    })
}

const NON_VALUE: NonValueRules = NonValueRules {
    always: &[
        "type",
        "import_statement",
        "import_from_statement",
        "global_statement",
        "nonlocal_statement",
        "dotted_name",
        "pattern_list",
        "case_pattern",
    ],
    is_field: &[("call", "function"), ("attribute", "attribute")],
    within_field: &[("keyword_argument", "name")],
};

fn python_non_value(parent: tree_sitter::Node<'_>, node: tree_sitter::Node<'_>) -> bool {
    non_value(parent, node, python_binding_target, &NON_VALUE)
}

const EXPRESSION_PARENTS: &[&str] = &[
    "return_statement",
    "binary_operator",
    "boolean_operator",
    "comparison_operator",
    "unary_operator",
    "not_operator",
    "argument_list",
    "list",
    "tuple",
    "set",
    "dictionary",
    "pair",
    "subscript",
    "conditional_expression",
    "await",
    "yield",
    "expression_statement",
    "assignment",
    "named_expression",
    "keyword_argument",
    "attribute",
    "if_statement",
    "while_statement",
    "default_parameter",
    "typed_default_parameter",
    "interpolation",
    "for_statement",
    "for_in_clause",
    "parenthesized_expression",
];

pub(super) fn python_read_position(node: tree_sitter::Node<'_>) -> bool {
    // Class namespaces are not lexical parents of method bodies. Class-body lookup is not
    // resolved by these bare non-local captures.
    if crate::languages::reads::enclosing_scope(node, python_scope)
        .is_some_and(|scope| scope.kind() == "class_definition")
    {
        return false;
    }
    crate::languages::reads::value_read(node, EXPRESSION_PARENTS, python_non_value, python_scope)
}
