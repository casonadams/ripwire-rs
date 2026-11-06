use super::java::{JVM_STD_MEMBERS, JVM_STD_QUALIFIERS};
use super::{ComplexityKinds, DialectGroup, LanguageSemantics, SyntaxKinds};

pub struct ScalaLanguage;
pub static SCALA_LANG: ScalaLanguage = ScalaLanguage;

const KINDS: ComplexityKinds = ComplexityKinds {
    paths: &["case_clause", "type_case_clause"],
    flat: &[],
    nesting: &[
        "if_expression",
        "for_expression",
        "while_expression",
        "do_while_expression",
        "catch_clause",
    ],
    nest_only: &["match_expression"],
    else_kinds: &["else"],
    else_bodies: &[],
    jumps: &["return_expression", "throw_expression"],
    logical_nodes: &[],
    binary_nodes: &["infix_expression"],
    logical_ops: &["&&", "||"],
};

const SYNTAX: SyntaxKinds = SyntaxKinds {
    scope_containers: &["trait_definition", "class_definition", "object_definition"],
    member_access: &["field_expression"],
    field_access: &["field_expression"],
    body_blocks: &["block", "template_body", "indented_block"],
    body_wrappers: &["parenthesized_expression"],
    function_defs: &["function_declaration", "function_definition"],
    non_local_declarations: &["function_declaration"],
    identifiers: &["identifier"],
    type_contexts: &["type_parameters", "type_arguments", "generic_type"],
    callable_wrappers: &["parenthesized_expression"],
    binding_patterns: &["tuple_pattern"],
    ..SyntaxKinds::NONE
};

const TYPED: super::typed::TypedSyntax = super::typed::TypedSyntax {
    classes: &["class_definition", "trait_definition", "object_definition"],
    supertype_clauses: &["extends_clause"],
    params: &["parameter"],
    functions: &["function_definition", "function_declaration", "lambda_expression"],
    locals: &["val_definition", "var_definition"],
    fields: &["val_definition", "var_definition"],
    declarators: &[],
    blocks: &["block", "indented_block", "template_body"],
    constructions: &["instance_expression"],
    inferred: &[],
    assignments: &[],
    calls: &[("call_expression", "function")],
    return_type_field: "return_type",
    superclass: Some(("extends_clause", "super")),
    declaration_name: Some("pattern"),
    class_params: &["class_parameter"],
    transparent_wrappers: &[],
    scoped_nesting: true,
};

impl LanguageSemantics for ScalaLanguage {
    fn nested_types_are_scoped(&self) -> bool {
        true
    }

    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &KINDS
    }

    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }

    super::impl_typed_class_hooks!(TYPED);
    fn overloads_by_arity(&self) -> bool {
        true
    }

    fn tag(&self) -> &'static str {
        "scala"
    }
    super::impl_grammar_method!(crate tree_sitter_scala::LANGUAGE, "tags.scm");

    fn extensions(&self) -> &'static [&'static str] {
        &["scala", "sc"]
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Jvm
    }

    fn std_member_names(&self) -> &'static [&'static str] {
        JVM_STD_MEMBERS
    }

    fn std_qualifier_names(&self) -> &'static [&'static str] {
        JVM_STD_QUALIFIERS
    }
}
