use super::java::{JVM_STD_MEMBERS, JVM_STD_QUALIFIERS};
use super::{ComplexityKinds, DialectGroup, LanguageSemantics, SyntaxKinds, VisibilityMode};

mod types;
pub struct KotlinLanguage;
pub static KOTLIN_LANG: KotlinLanguage = KotlinLanguage;

/// Kotlin stdlib scope functions and JVM metadata accessors. Every kotlinpoet site where an untyped call of one of
/// these bound to an in-tree method was a false edge, so an untyped receiver declines instead.
const KOTLIN_STD_MEMBERS: &[&str] = &[
    "apply",
    "asType",
    "getter",
    "mutable",
    "primaryConstructor",
    "setter",
];

const KINDS: ComplexityKinds = ComplexityKinds {
    paths: &["when_entry"],
    flat: &[],
    nesting: &[
        "if_expression",
        "for_statement",
        "while_statement",
        "do_while_statement",
        "catch_block",
    ],
    nest_only: &["when_expression"],
    else_kinds: &["else"],
    else_bodies: &["control_structure_body"],
    jumps: &["jump_expression"],
    logical_nodes: &[
        "conjunction_expression",
        "disjunction_expression",
        "elvis_expression",
    ],
    binary_nodes: &[],
    logical_ops: &[],
};

const SYNTAX: SyntaxKinds = SyntaxKinds {
    scope_containers: &[
        "class_declaration",
        "object_declaration",
        "companion_object",
    ],
    member_access: &["navigation_expression", "navigation_suffix"],
    body_blocks: &["function_body", "class_body", "enum_class_body"],
    body_wrappers: &[
        "variable_declaration",
        "property_declaration",
        "as_expression",
        "parenthesized_expression",
    ],
    function_defs: &["function_declaration"],
    non_local_declarations: &["function_declaration", "class_declaration"],
    identifiers: &["identifier", "simple_identifier"],
    type_contexts: &["type_parameters", "type_parameter", "type_arguments"],
    callable_wrappers: &["as_expression", "parenthesized_expression"],
    strict_param_lists: &["function_value_parameters"],
    ..SyntaxKinds::NONE
};

impl LanguageSemantics for KotlinLanguage {
    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &KINDS
    }

    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }

    fn member_receiver<'a>(&self, access: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
        if access.kind() == "navigation_suffix" {
            access.parent().and_then(|nav| nav.named_child(0))
        } else {
            None
        }
    }

    fn overloads_by_arity(&self) -> bool {
        true
    }

    fn read_bindings<'a>(
        &self,
        root: tree_sitter::Node<'_>,
        source: &'a [u8],
    ) -> super::reads::ReadBindings<'a> {
        types::typed_read_bindings(root, source)
    }

    fn implementations(
        &self,
        root: tree_sitter::Node<'_>,
        source: &[u8],
    ) -> Vec<super::Implementation> {
        types::class_implementations(root, source)
    }

    super::impl_class_dispatch_flags!();

    fn call_receiver_name<'a>(
        &self,
        receiver: tree_sitter::Node<'_>,
        source: &'a [u8],
    ) -> Option<&'a str> {
        let callee = receiver.named_child(0).filter(|callee| {
            receiver.kind() == "call_expression" && callee.kind() == "simple_identifier"
        })?;
        callee.utf8_text(source).ok()
    }

    fn nested_types_are_scoped(&self) -> bool {
        true
    }

    fn receiver_chain(&self, receiver: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
        types::receiver_chain(receiver, source)
    }

    fn is_std_or_builtin_member(&self, name: &str) -> bool {
        JVM_STD_MEMBERS.contains(&name) || KOTLIN_STD_MEMBERS.contains(&name)
    }

    fn tag(&self) -> &'static str {
        "kt"
    }

    fn lint_groups(&self) -> &'static [crate::languages::LintGroup] {
        &[crate::languages::LintGroup::Naming]
    }
    super::impl_grammar_method!(ffi tree_sitter_kotlin, include_str!("tags.scm"));

    fn lint_extra_rules(&self) -> &'static [&'static str] {
        &["redundant-parens"]
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["kt", "kts"]
    }

    fn display_color(&self) -> &'static str {
        "#a97bff"
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Jvm
    }

    fn visibility_mode(&self) -> VisibilityMode {
        VisibilityMode::ExplicitModifier
    }

    fn std_member_names(&self) -> &'static [&'static str] {
        JVM_STD_MEMBERS
    }

    fn std_qualifier_names(&self) -> &'static [&'static str] {
        JVM_STD_QUALIFIERS
    }
}
