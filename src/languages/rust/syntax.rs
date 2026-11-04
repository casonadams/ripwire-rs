use super::super::{ComplexityKinds, SyntaxKinds};

pub(super) const KINDS: ComplexityKinds = ComplexityKinds {
    paths: &["match_arm"],
    flat: &[],
    nesting: &[
        "if_expression",
        "for_expression",
        "while_expression",
        "loop_expression",
    ],
    nest_only: &["match_expression"],
    else_kinds: &["else_clause"],
    else_bodies: &[],
    jumps: &[
        "break_expression",
        "continue_expression",
        "return_expression",
    ],
    logical_nodes: &[],
    binary_nodes: &["binary_expression"],
    logical_ops: &["&&", "||"],
};

pub(super) const SYNTAX: SyntaxKinds = SyntaxKinds {
    scope_containers: &["mod_item", "trait_item"],
    scoped_paths: &["scoped_identifier", "scoped_type_identifier"],
    member_access: &["field_expression"],
    field_access: &["field_expression"],
    body_blocks: &["block", "declaration_list", "field_declaration_list"],
    body_wrappers: &["parenthesized_expression"],
    function_defs: &["function_item"],
    local_bindings: &["let_condition", "for_expression"],
    identifiers: &["identifier"],
    type_contexts: &[
        "type_parameters",
        "type_parameter",
        "type_arguments",
        "generic_type",
    ],
    callable_wrappers: &["parenthesized_expression"],
    binding_patterns: &[
        "tuple_pattern",
        "slice_pattern",
        "tuple_struct_pattern",
        "struct_pattern",
        "field_pattern",
        "mut_pattern",
        "ref_pattern",
    ],
    ..SyntaxKinds::NONE
};
