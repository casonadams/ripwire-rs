use tree_sitter::Node;

const BLOCKS: &[&str] = &[
    "statement_block",
    "for_statement",
    "for_in_statement",
    "catch_clause",
    "switch_body",
    "class_static_block",
];

const SCOPES: &[&str] = &[
    "function_declaration",
    "function_expression",
    "function",
    "generator_function_declaration",
    "generator_function",
    "arrow_function",
    "method_definition",
    "program",
];

/// Names a file binds as local variables or parameters, each with the byte span of the function or program
/// that scopes it: `const`/`let`/`var` declarators, parameters, `catch` parameters and `for` targets.
/// `let`, `const` and `catch` bindings are scoped to their block, `for` statement or clause; `var` and
/// parameters to their function.
pub(in crate::languages) fn local_binding_names(
    root: Node<'_>,
    source: &[u8],
) -> Vec<(String, u32, u32)> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let (names, block) = node_bindings(node, source);
        if !names.is_empty()
            && let Some((start, end)) = scope_span(node, block)
        {
            found.extend(names.into_iter().map(|name| (name, start, end)));
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    found.sort_unstable();
    found.dedup();
    found
}

/// The names `node` itself binds and whether they are scoped to a block rather than a function.
fn node_bindings(node: Node<'_>, source: &[u8]) -> (Vec<String>, bool) {
    let mut names = Vec::new();
    let mut block = false;
    match node.kind() {
        "variable_declarator" if !binds_a_module(node, source) && !defines_a_function(node) => {
            block = node
                .parent()
                .is_some_and(|parent| parent.kind() == "lexical_declaration");
            push_field(node, "name", source, &mut names);
        }
        "formal_parameters" => push_parameters(node, source, &mut names),
        "arrow_function" => push_field(node, "parameter", source, &mut names),
        "catch_clause" => {
            block = true;
            push_field(node, "parameter", source, &mut names);
        }
        "for_in_statement" => {
            block = node
                .child_by_field_name("kind")
                .and_then(|kind| kind.utf8_text(source).ok())
                .is_some_and(|kind| kind != "var");
            push_field(node, "left", source, &mut names);
        }
        _ => {}
    }
    (names, block)
}

fn push_field(node: Node<'_>, field: &str, source: &[u8], names: &mut Vec<String>) {
    if let Some(pattern) = node.child_by_field_name(field) {
        push_pattern(pattern, source, names);
    }
}

/// True for `const f = () => ..` and `const f = function () ..`: the declarator is itself an indexed function
/// definition, so calls to `f` in its scope bind it instead of being treated as calls of a local variable.
fn defines_a_function(declarator: Node<'_>) -> bool {
    declarator
        .child_by_field_name("value")
        .is_some_and(|value| matches!(value.kind(), "arrow_function" | "function_expression"))
}

/// True for `const x = require("m")`, `const x = require("m").default` and `const x = await import("m")`: that binds the module's export, so
/// the name is an import, not a local that shadows a definition elsewhere.
fn binds_a_module(declarator: Node<'_>, source: &[u8]) -> bool {
    let Some(mut value) = declarator.child_by_field_name("value") else {
        return false;
    };
    loop {
        let inner = match value.kind() {
            "await_expression" | "parenthesized_expression" => value.named_child(0),
            "member_expression" => value.child_by_field_name("object"),
            _ => break,
        };
        let Some(inner) = inner else {
            return false;
        };
        value = inner;
    }
    value.kind() == "call_expression"
        && value
            .child_by_field_name("function")
            .and_then(|function| function.utf8_text(source).ok())
            .is_some_and(|text| matches!(text, "require" | "import"))
}

/// The span that scopes a binding written at `node`: the nearest block when `block`, else the nearest
/// function or program. A parameter list or an arrow function's own parameter belongs to the function
/// around it.
fn scope_span(node: Node<'_>, block: bool) -> Option<(u32, u32)> {
    let mut current = node;
    loop {
        if SCOPES.contains(&current.kind()) || (block && BLOCKS.contains(&current.kind())) {
            return Some((current.start_byte() as u32, current.end_byte() as u32));
        }
        current = current.parent()?;
    }
}

fn push_pattern(pattern: Node<'_>, source: &[u8], names: &mut Vec<String>) {
    match pattern.kind() {
        "identifier" | "shorthand_property_identifier_pattern" => {
            if let Ok(text) = pattern.utf8_text(source) {
                names.push(text.to_string());
            }
        }
        "object_pattern"
        | "array_pattern"
        | "rest_pattern"
        | "assignment_pattern"
        | "pair_pattern"
        | "object_assignment_pattern" => {
            let mut cursor = pattern.walk();
            for child in pattern.named_children(&mut cursor) {
                let target = match child.kind() {
                    "pair_pattern" => child.child_by_field_name("value"),
                    "assignment_pattern" | "object_assignment_pattern" => {
                        child.child_by_field_name("left")
                    }
                    _ => Some(child),
                };
                if let Some(target) = target {
                    push_pattern(target, source, names);
                }
            }
        }
        _ => {}
    }
}

fn push_parameters(parameters: Node<'_>, source: &[u8], names: &mut Vec<String>) {
    let mut cursor = parameters.walk();
    for parameter in parameters.named_children(&mut cursor) {
        match parameter.kind() {
            "required_parameter" | "optional_parameter" => {
                if let Some(pattern) = parameter.child_by_field_name("pattern") {
                    push_pattern(pattern, source, names);
                }
            }
            _ => push_pattern(parameter, source, names),
        }
    }
}
