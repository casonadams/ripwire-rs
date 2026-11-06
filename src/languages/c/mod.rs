pub mod layout;
pub mod flags;
pub(crate) mod lint;

mod decl;
mod defs;
mod headers;
mod macros;
mod names;
mod aliases;

use super::{ComplexityKinds, DefCaptureGateContext, DialectGroup, LanguageSemantics, SyntaxKinds};
use std::collections::HashMap;

pub(super) use self::decl::is_c_internal_function;
pub(super) use self::defs::{
    c_family_declaration_ties, is_c_family_def_allowed, resolve_c_family_def_body_and_end,
};
pub(super) use self::headers::{refine_c_header_dialect, resolve_c_family_include};
pub(super) use self::macros::blank_c_family_macro_lines;
pub(super) use self::aliases::type_aliases;
pub(super) use self::names::{
    C_FAMILY_KEYWORDS, C_LIBRARY_FUNCTIONS, CPP_STD_MEMBERS, CPP_STD_QUALIFIERS,
};

pub struct CLanguage;
pub static C_LANG: CLanguage = CLanguage;

const KINDS: ComplexityKinds = ComplexityKinds {
    nesting: &[
        "if_statement",
        "for_statement",
        "while_statement",
        "do_statement",
        "conditional_expression",
    ],
    jumps: &[
        "break_statement",
        "continue_statement",
        "goto_statement",
        "return_statement",
    ],
    ..ComplexityKinds::C_STYLE
};

const SYNTAX: SyntaxKinds = SyntaxKinds {
    scope_containers: &["struct_specifier"],
    member_access: &["field_expression"],
    field_access: &["field_expression"],
    body_blocks: &[
        "compound_statement",
        "declaration_list",
        "field_declaration_list",
    ],
    body_wrappers: &["parenthesized_expression"],
    function_defs: &["function_definition"],
    function_declarators: &["function_declarator"],
    identifiers: &["identifier"],
    callables: &["function_declarator"],
    callable_wrappers: &["parenthesized_expression"],
    signature_holders: &["declaration"],
    binding_patterns: &[
        "pointer_declarator",
        "array_declarator",
        "parenthesized_declarator",
    ],
    ..SyntaxKinds::NONE
};

impl LanguageSemantics for CLanguage {
    crate::languages::c::flags::c_family_hooks!();

    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &KINDS
    }
    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }
    fn has_declarations(&self) -> bool {
        true
    }
    fn declaration_ties(&self, decl: &crate::domain::Symbol, def: &crate::domain::Symbol) -> bool {
        c_family_declaration_ties(decl, def)
    }
    fn tag(&self) -> &'static str {
        "c"
    }

    fn lint_groups(&self) -> &'static [crate::languages::LintGroup] {
        super::lint::C_FAMILY_LINT_GROUPS
    }
    super::impl_grammar_method!(crate tree_sitter_c::LANGUAGE, "tags.scm");

    fn lint_return_type<'a>(&self, sig: &'a str, name: &str) -> Option<&'a str> {
        super::lint_hooks::prefix_return_type(sig, name)
    }

    fn type_declaration_keywords(&self) -> &'static [&'static str] {
        &["typedef", "enum"]
    }

    fn display_color(&self) -> &'static str {
        "#555555"
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::CFamily
    }

    fn layout_model(&self) -> Option<fn(&str) -> Vec<crate::metrics::FieldLayout>> {
        Some(layout::parse_struct_fields)
    }

    fn refine_dialect(
        &self,
        base_lang: crate::domain::Lang,
        rel_path: &str,
        source_bytes: &[u8],
    ) -> crate::domain::Lang {
        refine_c_header_dialect(base_lang, rel_path, source_bytes)
    }

    /// In C a member call `x->f(...)` or `x.f(...)` can only reach a function-pointer field, never a
    /// free function, so the call is declined rather than bound by name to a same-named function.
    fn is_callable_field_access(&self, name_node: tree_sitter::Node<'_>) -> bool {
        let Some(member) = name_node.parent().filter(|p| p.kind() == "field_expression") else {
            return false;
        };
        member.parent().is_some_and(|call| {
            call.kind() == "call_expression" && call.child_by_field_name("function") == Some(member)
        })
    }

    fn is_builtin_keyword(&self, name: &str) -> bool {
        C_FAMILY_KEYWORDS.contains(&name)
    }

    fn std_member_names(&self) -> &'static [&'static str] {
        CPP_STD_MEMBERS
    }

    fn std_qualifier_names(&self) -> &'static [&'static str] {
        CPP_STD_QUALIFIERS
    }

    fn library_function_names(&self) -> &'static [&'static str] {
        C_LIBRARY_FUNCTIONS
    }

    fn cross_file_field_types(&self) -> bool {
        true
    }

    fn bodyless_types_are_forward_declarations(&self) -> bool {
        true
    }

    fn type_aliases(&self, root: tree_sitter::Node<'_>, source: &[u8]) -> Vec<(String, String)> {
        type_aliases(root, source, &[])
    }

    fn blank_error_macros(&self, source_bytes: &[u8]) -> Option<Vec<u8>> {
        blank_c_family_macro_lines(source_bytes)
    }

    fn is_def_capture_allowed(&self, ctx: &DefCaptureGateContext<'_>) -> bool {
        is_c_family_def_allowed(ctx, false)
    }

    fn resolve_def_body_and_end<'a>(
        &self,
        def_node: Option<tree_sitter::Node<'a>>,
        default_end: usize,
    ) -> (Option<tree_sitter::Node<'a>>, usize) {
        resolve_c_family_def_body_and_end(def_node, default_end)
    }

    fn resolve_import_target(
        &self,
        importer_path: &str,
        raw_target: &str,
        file_index: &HashMap<&str, u32>,
    ) -> Option<u32> {
        resolve_c_family_include(importer_path, raw_target, file_index)
    }
    fn supports_internal_linkage(&self) -> bool {
        true
    }

    fn is_internal_function(
        &self,
        root: tree_sitter::Node<'_>,
        name_byte: usize,
        source: &[u8],
    ) -> bool {
        is_c_internal_function(root, name_byte, source, false)
    }
}
