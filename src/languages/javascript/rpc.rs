use crate::domain::{Lang, RefRole};
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
            if is_function_name(name) {
                refs.push(sql_call(name, argument));
            }
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
}

fn is_function_name(name: &str) -> bool {
    name.bytes()
        .next()
        .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
        && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn sql_call(name: &str, node: Node<'_>) -> RawRef {
    let line = node.start_position().row as u32 + 1;
    RawRef {
        callee_name: name.to_string(),
        qualifier: None,
        line,
        name_line: line,
        start_byte: node.start_byte(),
        role: RefRole::Call,
        arg_count: 0,
        arg_count_known: false,
        lang: Some(Lang::Sql),
        receiver: None,
        receiver_type: None,
        receiver_chain: None,
        receiver_file: None,
        callable_field: false,
        macro_arg: false,
        member_call: false,
        path_call: false,
    }
}
