use super::scope::{rust_binding_target, rust_scope};
use super::types::nominal_name;
use crate::languages::reads::{NonValueRules, non_value};

/// Function names are snake_case. CamelCase names in macro arguments are often patterns
/// (`matches!(x, Kind::Mode(_))`) or constructors, so only snake_case names count as calls.
pub(super) fn is_function_name(name: &str) -> bool {
    name.starts_with(|c: char| c.is_ascii_lowercase() || c == '_')
}

/// A bare or path call written inside a macro argument list: an identifier directly followed by a parenthesized
/// token tree, as in `assert_eq!(alpha(2), 4)` or `assert_eq!(opts::fetch(x), y)`. Attribute arguments
/// (`derive(Debug)`), `macro_rules!` bodies (no call until expansion), declarations (`fn name(`, `struct Name(`),
/// and method calls (`x.name(`, whose receiver the token tree does not type) are not calls.
pub(super) fn is_macro_argument_call(node: tree_sitter::Node<'_>) -> bool {
    let parenthesized = node.next_sibling().is_some_and(|next| {
        next.kind() == "token_tree" && next.child(0).is_some_and(|open| open.kind() == "(")
    });
    let declared = node.prev_sibling().is_some_and(|prev| match prev.kind() {
        "fn" | "struct" | "enum" | "union" | "." => true,
        "::" => macro_path_segments(node).is_empty(),
        _ => false,
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

/// The segments of the path a call in a macro argument list is reached through, first segment first. Macro tokens
/// are flat, so the path is read backwards from the call's name: `a::b::f(` gives `a` and `b`, `Type::<T>::f(`
/// gives `Type`. Empty when no `::` precedes the name.
fn macro_path_segments(node: tree_sitter::Node<'_>) -> Vec<tree_sitter::Node<'_>> {
    let mut segments = Vec::new();
    let mut separator = node.prev_sibling().filter(|s| s.kind() == "::");
    while let Some(sep) = separator {
        let Some(mut segment) = sep.prev_sibling() else {
            break;
        };
        if segment.kind() == ">" {
            let Some(owner) = turbofish_owner(segment) else {
                break;
            };
            segment = owner;
        }
        if !matches!(segment.kind(), "identifier" | "self" | "super" | "crate") {
            break;
        }
        segments.push(segment);
        separator = segment.prev_sibling().filter(|s| s.kind() == "::");
    }
    segments.reverse();
    segments
}

/// The path segment before the turbofish `::<..>` whose closing `>` is `close`.
fn turbofish_owner(close: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let mut depth = 0usize;
    let mut token = Some(close);
    let open = loop {
        let current = token?;
        match current.kind() {
            ">" => depth += 1,
            "<" => {
                depth -= 1;
                if depth == 0 {
                    break current;
                }
            }
            _ => {}
        }
        token = current.prev_sibling();
    };
    open.prev_sibling()
        .filter(|s| s.kind() == "::")?
        .prev_sibling()
        .filter(|s| s.kind() == "identifier")
}

/// The qualifier of a call in a macro argument list, with the scoped-path rule: a type segment under a module
/// path names the type (`crate::m::Type::f(` gives `Type`), `m::f(` gives `m`, and `Self::f(` gives the enclosing
/// impl's type, or `Self` outside an impl.
pub(super) fn macro_path_qualifier(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let texts = macro_path_segments(node)
        .into_iter()
        .map(|segment| segment.utf8_text(source).ok())
        .collect::<Option<Vec<&str>>>()?;
    let last = *texts.last()?;
    if texts.len() >= 2 && is_type_segment(last) {
        return Some(last.to_string());
    }
    if last == "Self" {
        return Some(enclosing_impl_type(node, source).unwrap_or_else(|| "Self".to_string()));
    }
    Some(texts.join("::"))
}

fn is_type_segment(segment: &str) -> bool {
    segment.starts_with(|c: char| c.is_ascii_uppercase())
        && segment != "Self"
        && segment
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn enclosing_impl_type(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let impl_item =
        std::iter::successors(node.parent(), |n| n.parent()).find(|n| n.kind() == "impl_item")?;
    nominal_name(source, impl_item.child_by_field_name("type")?).map(str::to_string)
}

const NON_VALUE: NonValueRules = NonValueRules {
    always: &[
        "type_arguments",
        "type_parameters",
        "use_declaration",
        "macro_invocation",
        "token_tree",
        "attribute_item",
        "inner_attribute_item",
    ],
    is_field: &[("call_expression", "function"), ("scoped_identifier", "path")],
    within_field: &[("assignment_expression", "left"), ("compound_assignment_expr", "left")],
};

fn rust_non_value(parent: tree_sitter::Node<'_>, node: tree_sitter::Node<'_>) -> bool {
    non_value(parent, node, rust_binding_target, &NON_VALUE)
}


const EXPRESSION_PARENTS: &[&str] = &[
    "binary_expression",
    "return_expression",
    "arguments",
    "array_expression",
    "tuple_expression",
    "reference_expression",
    "unary_expression",
    "index_expression",
    "field_expression",
    "let_declaration",
    "const_item",
    "static_item",
    "assignment_expression",
    "compound_assignment_expr",
    "if_expression",
    "while_expression",
    "match_expression",
    "closure_expression",
    "parenthesized_expression",
    "expression_statement",
    "block",
    "field_initializer",
    "shorthand_field_initializer",
    "scoped_identifier",
];

pub(super) fn rust_read_position(node: tree_sitter::Node<'_>) -> bool {
    if node.parent().is_some_and(|parent| {
        parent.kind() == "scoped_identifier"
            && parent.parent().is_some_and(|call| {
                call.kind() == "call_expression"
                    && call.child_by_field_name("function") == Some(parent)
            })
    }) {
        return false;
    }
    crate::languages::reads::value_read(node, EXPRESSION_PARENTS, rust_non_value, rust_scope)
}
