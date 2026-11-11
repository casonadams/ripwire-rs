use crate::ingest::RawRef;
use tree_sitter::Node;

/// Calls of SQL functions written as `client.rpc("name", ...)`, the Supabase and PostgREST way to call a
/// database function by name: the string is a reference to the function `name`.
pub(super) fn rpc_call_refs(root: Node<'_>, source: &[u8], refs: &mut Vec<RawRef>) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "call"
            && let Some(function) = node.child_by_field_name("function")
            && function.kind() == "attribute"
            && function
                .child_by_field_name("attribute")
                .and_then(|attribute| attribute.utf8_text(source).ok())
                == Some("rpc")
            && let Some(argument) = node
                .child_by_field_name("arguments")
                .and_then(|arguments| arguments.named_child(0))
            && argument.kind() == "string"
            && !has_interpolation(argument)
            && let Ok(text) = argument.utf8_text(source)
        {
            let name = text.trim_start_matches(['r', 'R', 'b', 'B', 'u', 'U']);
            let name = name.trim_matches(['"', '\'']);
            if crate::languages::sql::embedded::is_valid_sql_ident(name) {
                refs.push(crate::languages::sql::embedded::sql_call_ref(
                    name,
                    argument.start_byte(),
                    argument.start_position().row as u32 + 1,
                ));
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
}

fn has_interpolation(string: Node<'_>) -> bool {
    let mut cursor = string.walk();
    string
        .named_children(&mut cursor)
        .any(|child| child.kind() == "interpolation")
}
