use tree_sitter::Node;

/// `(bound name, module path)` for each name a `from M import name [as alias]` statement may bind as a
/// submodule: the path joins `M` and `name` (`from src import real` names `src.real`, `from . import x`
/// names `.x`). Whether such a module exists is for the importer's resolver to say.
pub(super) fn submodule_imports(root: Node<'_>, source: &[u8]) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "import_from_statement" {
            collect_statement(node, source, &mut found);
            continue;
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    found.sort_unstable();
    found.dedup();
    found
}

fn collect_statement(statement: Node<'_>, source: &[u8], found: &mut Vec<(String, String)>) {
    let Some(module) = statement
        .child_by_field_name("module_name")
        .and_then(|node| node.utf8_text(source).ok())
    else {
        return;
    };
    let mut cursor = statement.walk();
    for name in statement.children_by_field_name("name", &mut cursor) {
        let (imported, bound) = if name.kind() == "aliased_import" {
            let text = |field| {
                name.child_by_field_name(field)
                    .and_then(|node| node.utf8_text(source).ok())
            };
            match (text("name"), text("alias")) {
                (Some(imported), Some(alias)) => (imported, alias),
                _ => continue,
            }
        } else {
            match name.utf8_text(source) {
                Ok(text) => (text, text),
                Err(_) => continue,
            }
        };
        let separator = if module.ends_with('.') { "" } else { "." };
        found.push((bound.to_string(), format!("{module}{separator}{imported}")));
    }
}
