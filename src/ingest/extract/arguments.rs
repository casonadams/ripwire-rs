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
