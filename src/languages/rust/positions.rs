use super::scope::{rust_binding_target, rust_scope};

/// Function names are snake_case. CamelCase names in macro arguments are often patterns
/// (`matches!(x, Kind::Mode(_))`) or constructors, so only snake_case names count as calls.
pub(super) fn is_function_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
}

/// A bare call written inside a macro argument list: an identifier directly followed by a parenthesized
/// token tree, as in `assert_eq!(alpha(2), 4)`. Attribute arguments (`derive(Debug)`), `macro_rules!`
/// bodies (no call until expansion), declarations (`fn name(`, `struct Name(`), and method or path
/// calls (`x.name(`, `m::name(`, whose receiver or module the token tree does not type) are not calls.
pub(super) fn is_macro_argument_call(node: tree_sitter::Node<'_>) -> bool {
    let parenthesized = node.next_sibling().is_some_and(|next| {
        next.kind() == "token_tree" && next.child(0).is_some_and(|open| open.kind() == "(")
    });
    let declared = node.prev_sibling().is_some_and(|prev| {
        matches!(
            prev.kind(),
            "fn" | "struct" | "enum" | "union" | "." | "::"
        )
    });
    let mut ancestor = node.parent();
    let in_metadata_or_rules = std::iter::from_fn(|| {
        let current = ancestor?;
        ancestor = current.parent();
        Some(current.kind())
    })
    .any(|kind| matches!(kind, "attribute_item" | "inner_attribute_item" | "macro_definition"));
    parenthesized && !declared && !in_metadata_or_rules
}

fn rust_non_value(parent: tree_sitter::Node<'_>, node: tree_sitter::Node<'_>) -> bool {
    if rust_binding_target(parent, node) {
        return true;
    }
    match parent.kind() {
        "type_arguments"
        | "type_parameters"
        | "use_declaration"
        | "macro_invocation"
        | "token_tree"
        | "attribute_item"
        | "inner_attribute_item" => true,
        "call_expression" => parent.child_by_field_name("function") == Some(node),
        "assignment_expression" | "compound_assignment_expr" => {
            super::super::reads::in_field(parent, node, "left")
        }
        "scoped_identifier" => parent.child_by_field_name("path") == Some(node),
        _ => false,
    }
}

pub(super) fn rust_read_position(node: tree_sitter::Node<'_>) -> bool {
    let Some(parent) = node.parent() else {
        return false;
    };
    if parent.kind() == "scoped_identifier"
        && parent.parent().is_some_and(|call| {
            call.kind() == "call_expression" && call.child_by_field_name("function") == Some(parent)
        })
    {
        return false;
    }
    let expression = matches!(
        parent.kind(),
        "binary_expression"
            | "return_expression"
            | "arguments"
            | "array_expression"
            | "tuple_expression"
            | "reference_expression"
            | "unary_expression"
            | "index_expression"
            | "field_expression"
            | "let_declaration"
            | "const_item"
            | "static_item"
            | "assignment_expression"
            | "compound_assignment_expr"
            | "if_expression"
            | "while_expression"
            | "match_expression"
            | "closure_expression"
            | "parenthesized_expression"
            | "expression_statement"
            | "block"
            | "field_initializer"
            | "shorthand_field_initializer"
            | "scoped_identifier"
    );
    expression && super::super::reads::value_position(node, rust_non_value, rust_scope)
}
