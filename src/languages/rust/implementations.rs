use super::types::{Constraints, nominal_name, scope_constraints};

fn implementation_fact(
    node: tree_sitter::Node<'_>,
    source: &[u8],
) -> Option<super::super::Implementation> {
    let subject = nominal_name(source, node.child_by_field_name("type")?)?;
    let mut bounds = Constraints::default();
    let generic_subject = scope_constraints(node, subject, source, &mut bounds);
    Some(super::super::Implementation {
        trait_path: node
            .child_by_field_name("trait")
            .and_then(|ty| nominal_name(source, ty))
            .map(str::to_string),
        subject: subject.to_string(),
        subject_bound: if generic_subject {
            bounds.unique().map(str::to_string)
        } else {
            None
        },
        generic_subject,
        start_byte: node.start_byte(),
        end_byte: node.end_byte(),
    })
}

pub(super) fn collect_rust_implementations(
    root: tree_sitter::Node<'_>,
    source: &[u8],
) -> Vec<super::super::Implementation> {
    super::super::reads::nodes(root)
        .filter(|node| node.kind() == "impl_item")
        .filter_map(|node| implementation_fact(node, source))
        .collect()
}