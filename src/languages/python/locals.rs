use tree_sitter::Node;

/// Names a file binds as local variables or parameters, each with the byte span of the function, lambda
/// or module that scopes it: assignment, `for` and `with ... as` targets, walrus names, `except ... as`
/// names and parameters. Class-body bindings are not visible to the methods, so they are left out.
pub(super) fn local_binding_names(root: Node<'_>, source: &[u8]) -> Vec<(String, u32, u32)> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        let mut names = Vec::new();
        match node.kind() {
            "assignment" | "augmented_assignment" | "for_statement" | "for_in_clause" => {
                if let Some(target) = node.child_by_field_name("left") {
                    push_targets(target, source, &mut names);
                }
            }
            "named_expression" => {
                if let Some(name) = node.child_by_field_name("name") {
                    push_targets(name, source, &mut names);
                }
            }
            "as_pattern" => {
                let mut cursor = node.walk();
                for alias in node.children_by_field_name("alias", &mut cursor) {
                    push_targets(alias, source, &mut names);
                }
            }
            "parameters" | "lambda_parameters" => push_parameters(node, source, &mut names),
            _ => {}
        }
        if !names.is_empty()
            && let Some((start, end)) = scope_span(node)
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

/// The span of the nearest function, lambda or module around `node`; `None` inside a class body.
fn scope_span(node: Node<'_>) -> Option<(u32, u32)> {
    let mut current = node;
    loop {
        match current.kind() {
            "function_definition" | "lambda" | "module" => {
                return Some((current.start_byte() as u32, current.end_byte() as u32));
            }
            "class_definition" => return None,
            _ => current = current.parent()?,
        }
    }
}

fn push_targets(target: Node<'_>, source: &[u8], names: &mut Vec<String>) {
    match target.kind() {
        "identifier" => {
            if let Ok(text) = target.utf8_text(source) {
                names.push(text.to_string());
            }
        }
        "as_pattern_target" | "pattern_list" | "tuple_pattern" | "list_pattern"
        | "list_splat_pattern" | "parenthesized_expression" => {
            let mut cursor = target.walk();
            for child in target.named_children(&mut cursor) {
                push_targets(child, source, names);
            }
        }
        _ => {}
    }
}

fn push_parameters(parameters: Node<'_>, source: &[u8], names: &mut Vec<String>) {
    let mut cursor = parameters.walk();
    for parameter in parameters.named_children(&mut cursor) {
        let named = parameter.child_by_field_name("name");
        match parameter.kind() {
            "default_parameter" | "typed_default_parameter" => {
                if let Some(name) = named {
                    push_targets(name, source, names);
                }
            }
            "typed_parameter" | "list_splat_pattern" | "dictionary_splat_pattern" => {
                let mut inner = parameter.walk();
                if let Some(first) = parameter.named_children(&mut inner).next() {
                    push_targets(first, source, names);
                }
            }
            _ => push_targets(parameter, source, names),
        }
    }
}
