use super::{ComplexityKinds, DialectGroup, LanguageSemantics, SyntaxKinds, VisibilityMode};
use crate::domain::Lang;

pub struct PhpLanguage;
pub static PHP_LANG: PhpLanguage = PhpLanguage;

const PHP_STD_MEMBERS: &[&str] = &[
    "count",
    "sizeof",
    "in_array",
    "array_map",
    "array_filter",
    "array_merge",
    "array_keys",
    "array_values",
    "array_push",
    "array_pop",
    "array_shift",
    "array_slice",
    "implode",
    "explode",
    "trim",
    "strlen",
    "strpos",
    "substr",
    "str_replace",
    "strtolower",
    "strtoupper",
    "sprintf",
    "json_encode",
    "json_decode",
];

const KINDS: ComplexityKinds = ComplexityKinds {
    paths: &["case_statement", "match_conditional_expression"],
    flat: &["else_if_clause"],
    nesting: &[
        "if_statement",
        "for_statement",
        "foreach_statement",
        "while_statement",
        "do_statement",
        "catch_clause",
        "conditional_expression",
    ],
    nest_only: &["switch_statement", "match_expression"],
    else_kinds: &["else_clause"],
    else_bodies: &[],
    jumps: &[
        "break_statement",
        "continue_statement",
        "goto_statement",
        "return_statement",
        "throw_expression",
    ],
    logical_nodes: &[],
    binary_nodes: &["binary_expression"],
    logical_ops: &["&&", "||", "and", "or"],
};

const SYNTAX: SyntaxKinds = SyntaxKinds {
    scope_containers: &[
        "trait_declaration",
        "class_declaration",
        "enum_declaration",
        "interface_declaration",
        "namespace_definition",
    ],
    member_access: &[
        "member_access_expression",
        "member_call_expression",
        "nullsafe_member_call_expression",
    ],
    body_blocks: &["compound_statement", "declaration_list"],
    body_wrappers: &[
        "property_declaration",
        "arrow_function",
        "parenthesized_expression",
    ],
    function_defs: &["function_definition"],
    non_local_declarations: &[
        "class_declaration",
        "interface_declaration",
        "enum_declaration",
    ],
    identifiers: &["variable_name", "name"],
    callables: &["arrow_function"],
    callable_wrappers: &["parenthesized_expression"],
    scoped_paths: &["scoped_call_expression"],
    ..SyntaxKinds::NONE
};

const TYPED: super::typed::TypedSyntax = super::typed::TypedSyntax {
    classes: &[
        "class_declaration",
        "interface_declaration",
        "trait_declaration",
        "enum_declaration",
    ],
    supertype_clauses: &["base_clause", "class_interface_clause"],
    params: &["simple_parameter", "property_promotion_parameter"],
    functions: &[
        "method_declaration",
        "function_definition",
        "anonymous_function",
        "arrow_function",
    ],
    locals: &[],
    fields: &["property_declaration"],
    declarators: &["property_element"],
    blocks: &["compound_statement"],
    constructions: &["object_creation_expression"],
    inferred: &[],
    assignments: &["assignment_expression"],
    calls: &[("function_call_expression", "function")],
    return_type_field: "return_type",
    superclass: None,
    declaration_name: None,
    class_params: &[],
    transparent_wrappers: &[],
    scoped_nesting: false,
};

impl LanguageSemantics for PhpLanguage {
    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &KINDS
    }

    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }

    fn receiver_text<'a>(&self, text: &'a str) -> std::borrow::Cow<'a, str> {
        if !text.contains('$') && !text.contains("->") {
            return std::borrow::Cow::Borrowed(text);
        }
        std::borrow::Cow::Owned(text.replace("?->", ".").replace("->", ".").replace('$', ""))
    }

    super::impl_typed_class_hooks!(TYPED);

    fn tag(&self) -> &'static str {
        "php"
    }

    fn lint_groups(&self) -> &'static [crate::languages::LintGroup] {
        &[crate::languages::LintGroup::Naming]
    }
    super::impl_grammar_method!(ffi tree_sitter_php, include_str!("tags.scm"));

    fn lint_extra_rules(&self) -> &'static [&'static str] {
        &[
            "c-style-cast",
            "redundant-parens",
            "do-while",
            "self-assign",
            "unreachable-code",
        ]
    }

    fn type_declaration_keywords(&self) -> &'static [&'static str] {
        &["enum"]
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["php", "phtml"]
    }

    fn display_color(&self) -> &'static str {
        "#4f5d95"
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Isolated(Lang::Php)
    }

    fn uses_hash_comments(&self) -> bool {
        true
    }

    fn visibility_mode(&self) -> VisibilityMode {
        VisibilityMode::ExplicitModifier
    }

    fn std_member_names(&self) -> &'static [&'static str] {
        PHP_STD_MEMBERS
    }
}
