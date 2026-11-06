use super::types::RawRef;
use crate::domain::{Lang, RefRole};

fn is_macro_call_keyword(ident: &str, lang: Lang) -> bool {
    lang.semantics().is_builtin_keyword(ident)
        || matches!(
            ident,
            "if" | "for" | "while" | "switch" | "return" | "defined" | "typeof"
        )
}

pub(super) fn scan_macro_body_calls(
    def_node: Option<tree_sitter::Node<'_>>,
    macro_name: &str,
    line: u32,
    start_byte: usize,
    lang: Lang,
    source_bytes: &[u8],
    refs: &mut Vec<RawRef>,
) {
    let Some(val_node) = def_node.and_then(|n| n.child_by_field_name("value")) else {
        return;
    };
    let Ok(text) = val_node.utf8_text(source_bytes) else {
        return;
    };
    for seg in text.split('(').collect::<Vec<_>>().windows(2) {
        let prefix = seg[0].trim_end();
        let ident: &str = prefix
            .rsplit(|c: char| !c.is_ascii_alphanumeric() && c != '_')
            .next()
            .unwrap_or("");
        let valid = ident.len() >= 2
            && ident != macro_name
            && ident
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_alphabetic() || b == b'_')
            && !is_macro_call_keyword(ident, lang);
        if valid {
            refs.push(RawRef {
                callee_name: ident.to_string(),
                qualifier: None,
                line,
                name_line: line,
                start_byte: start_byte.saturating_add(1),
                role: RefRole::Call,
                arg_count: 0,
                arg_count_known: false,
                lang: None,
                receiver: None,
                receiver_type: None,
                receiver_chain: None,
                callable_field: false,
                macro_arg: false,
                member_call: false,
                path_call: false,
            });
        }
    }
}
