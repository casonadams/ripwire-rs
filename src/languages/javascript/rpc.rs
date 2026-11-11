use crate::ingest::RawRef;
use tree_sitter::Node;

/// Calls of SQL functions written as `client.rpc("name", ...)`, the Supabase and PostgREST way to call a
/// database function by name: the string is a reference to the function `name`.
pub(in crate::languages) fn rpc_call_refs(root: Node<'_>, source: &[u8], refs: &mut Vec<RawRef>) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression"
            && let Some(function) = node.child_by_field_name("function")
            && function.kind() == "member_expression"
            && function
                .child_by_field_name("property")
                .and_then(|property| property.utf8_text(source).ok())
                == Some("rpc")
            && let Some(argument) = node
                .child_by_field_name("arguments")
                .and_then(|arguments| arguments.named_child(0))
            && matches!(argument.kind(), "string" | "template_string")
            && let Ok(text) = argument.utf8_text(source)
        {
            let name = text.trim_matches(|c| matches!(c, '"' | '\'' | '`'));
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
