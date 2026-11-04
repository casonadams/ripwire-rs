/// Ruby passes trailing `key => value` pairs as one hash argument.
fn count_arguments_in_list(child: tree_sitter::Node<'_>) -> u16 {
    let mut count = 0u16;
    let mut in_pairs = false;
    let mut cursor = child.walk();
    for arg in child
        .named_children(&mut cursor)
        .filter(|arg| !arg.kind().contains("comment"))
    {
        let is_pair = arg.kind() == "pair";
        if !(is_pair && in_pairs) {
            count = count.saturating_add(1);
        }
        in_pairs = is_pair;
    }
    count
}

fn find_arguments_child(parent: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    for i in 0..parent.child_count() {
        if let Some(child) = parent.child(i) {
            let ckind = child.kind();
            if ckind.contains("argument") || ckind == "args" {
                return Some(child);
            }
        }
    }
    None
}

pub(super) fn count_call_arguments(name_node: tree_sitter::Node<'_>) -> (u16, bool) {
    if let Some(counted) = count_macro_token_arguments(name_node) {
        return counted;
    }
    let mut curr = name_node.parent();
    while let Some(parent) = curr {
        let kind = parent.kind();
        if kind.contains("call") || kind == "invocation" || kind == "method_invocation" {
            if let Some(args_node) = find_arguments_child(parent) {
                return (count_arguments_in_list(args_node), true);
            }
            break;
        }
        curr = parent.parent();
    }
    (0, false)
}

/// A call written inside a macro's token tree is not parsed: the parenthesized token tree after its name
/// holds its arguments. The expression enclosing the macro belongs to an outer call, not to this one.
fn count_macro_token_arguments(name_node: tree_sitter::Node<'_>) -> Option<(u16, bool)> {
    if name_node.parent()?.kind() != "token_tree" {
        return None;
    }
    let args = name_node.next_sibling().filter(|next| {
        next.kind() == "token_tree" && next.child(0).is_some_and(|open| open.kind() == "(")
    })?;
    let mut count = 0u16;
    let mut in_argument = false;
    let mut cursor = args.walk();
    for child in args.children(&mut cursor).skip(1) {
        match child.kind() {
            ")" => break,
            "," => {
                count = count.saturating_add(u16::from(in_argument));
                in_argument = false;
            }
            kind if kind.contains("comment") => {}
            _ => in_argument = true,
        }
    }
    Some((count.saturating_add(u16::from(in_argument)), true))
}
