use super::binding::{javascript_binding_target, javascript_scope};
use crate::languages::reads::{NonValueRules, non_value};

const NON_VALUE: NonValueRules = NonValueRules {
    always: &[
        "type_annotation",
        "type_arguments",
        "type_parameters",
        "import_statement",
        "jsx_opening_element",
        "jsx_closing_element",
        "jsx_self_closing_element",
        "update_expression",
    ],
    is_field: &[
        ("call_expression", "function"),
        ("new_expression", "function"),
        ("new_expression", "constructor"),
    ],
    within_field: &[
        ("assignment_expression", "left"),
        ("augmented_assignment_expression", "left"),
        ("pair", "key"),
    ],
};

fn javascript_non_value(parent: tree_sitter::Node<'_>, node: tree_sitter::Node<'_>) -> bool {
    non_value(parent, node, javascript_binding_target, &NON_VALUE)
}

const EXPRESSION_PARENTS: &[&str] = &[
    "return_statement",
    "binary_expression",
    "ternary_expression",
    "arguments",
    "array",
    "object",
    "pair",
    "subscript_expression",
    "member_expression",
    "expression_statement",
    "unary_expression",
    "await_expression",
    "yield_expression",
    "spread_element",
    "template_substitution",
    "variable_declarator",
    "assignment_expression",
    "augmented_assignment_expression",
    "parenthesized_expression",
    "if_statement",
    "while_statement",
    "for_statement",
    "for_in_statement",
    "non_null_expression",
    "as_expression",
    "satisfies_expression",
    "assignment_pattern",
    "required_parameter",
    "optional_parameter",
    "jsx_expression",
    "throw_statement",
];

pub(crate) fn javascript_read_position(
    node: tree_sitter::Node<'_>,
    ancestors: &[tree_sitter::Node<'_>],
    source: &[u8],
) -> bool {
    if is_write_target_root(node, ancestors, source) {
        return false;
    }
    let Some(&parent) = ancestors.first() else {
        return false;
    };
    let expression =
        EXPRESSION_PARENTS.contains(&parent.kind()) || is_arrow_expression_body(parent, node);
    expression
        && crate::languages::reads::value_position(
            node,
            ancestors,
            javascript_non_value,
            javascript_scope,
        )
}

/// Methods that change their receiver in place: a call on a binding is a mutation of the object it holds.
const MUTATING_METHODS: &[&str] = &[
    "add", "clear", "copyWithin", "delete", "fill", "pop", "push", "reverse", "set", "shift", "sort",
    "splice", "unshift",
];

/// True when `node` is the root of a write target: `x` in `x = v`, `x += v`, `x++`, the object of `x.p = v`,
/// `x[i] = v`, or `delete x.p`, or the receiver of an in-place method call `x.push(v)`. Such an identifier is a
/// write, not a read of its value.
pub(crate) fn is_write_target_root(
    node: tree_sitter::Node<'_>,
    ancestors: &[tree_sitter::Node<'_>],
    source: &[u8],
) -> bool {
    let mut top = node;
    let mut depth = 0;
    while let Some(&parent) = ancestors.get(depth) {
        if !matches!(parent.kind(), "member_expression" | "subscript_expression")
            || parent.child_by_field_name("object") != Some(top)
        {
            break;
        }
        top = parent;
        depth += 1;
    }
    let Some(&parent) = ancestors.get(depth) else {
        return false;
    };
    match parent.kind() {
        "assignment_expression" | "augmented_assignment_expression" => {
            parent.child_by_field_name("left") == Some(top)
        }
        "update_expression" => true,
        "unary_expression" => parent.child(0).is_some_and(|op| op.kind() == "delete"),
        "call_expression" => {
            top.kind() == "member_expression"
                && parent.child_by_field_name("function") == Some(top)
                && top
                    .child_by_field_name("property")
                    .and_then(|property| property.utf8_text(source).ok())
                    .is_some_and(|name| MUTATING_METHODS.contains(&name))
        }
        _ => false,
    }
}

/// The body of an expression-bodied arrow (`() => x`): its value is returned, so the identifier is read.
fn is_arrow_expression_body(parent: tree_sitter::Node<'_>, node: tree_sitter::Node<'_>) -> bool {
    parent.kind() == "arrow_function" && parent.child_by_field_name("body") == Some(node)
}
