use super::c::{
    C_FAMILY_KEYWORDS, CPP_STD_MEMBERS, CPP_STD_QUALIFIERS, blank_c_family_macro_lines,
    is_c_family_def_allowed, resolve_c_family_def_body_and_end, resolve_c_family_include,
};
use super::{ComplexityKinds, DefCaptureGateContext, DialectGroup, LanguageSemantics, SyntaxKinds};
use std::collections::HashMap;

pub struct CudaLanguage;
pub static CUDA_LANG: CudaLanguage = CudaLanguage;

const KINDS: ComplexityKinds = ComplexityKinds {
    nesting: &[
        "if_statement",
        "for_statement",
        "for_range_loop",
        "while_statement",
        "do_statement",
        "catch_clause",
        "conditional_expression",
    ],
    jumps: &[
        "break_statement",
        "continue_statement",
        "goto_statement",
        "return_statement",
        "throw_statement",
        "co_return_statement",
    ],
    ..ComplexityKinds::C_STYLE
};

impl LanguageSemantics for CudaLanguage {
    crate::languages::c::flags::c_family_hooks!();

    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &KINDS
    }
    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &super::cpp::SYNTAX
    }
    fn has_declarations(&self) -> bool {
        true
    }
    fn declaration_ties(&self, decl: &crate::domain::Symbol, def: &crate::domain::Symbol) -> bool {
        super::c::c_family_declaration_ties(decl, def)
    }
    fn is_specialization_of(&self, spec: &str, primary: &str) -> bool {
        super::cpp::is_template_specialization(spec, primary)
    }
    fn extract_custom_scope(
        &self,
        parent: tree_sitter::Node<'_>,
        _name_node: tree_sitter::Node<'_>,
        source_bytes: &[u8],
    ) -> Option<String> {
        super::cpp::specialization_scope(parent, source_bytes)
    }
    fn tag(&self) -> &'static str {
        "cuda"
    }
    super::impl_grammar_method!(crate tree_sitter_cuda::LANGUAGE, "tags.scm");

    fn lint_groups(&self) -> &'static [crate::languages::LintGroup] {
        super::lint::C_FAMILY_LINT_GROUPS
    }

    fn lint_return_type<'a>(&self, sig: &'a str, name: &str) -> Option<&'a str> {
        super::lint_hooks::prefix_return_type(sig, name)
    }

    fn type_declaration_keywords(&self) -> &'static [&'static str] {
        &["typedef", "enum", "using"]
    }

    fn supports_cache_lint(&self) -> bool {
        true
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["cu", "cuh"]
    }

    fn display_color(&self) -> &'static str {
        "#3a87ad"
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::CFamily
    }

    fn layout_model(&self) -> Option<fn(&str) -> Vec<crate::metrics::FieldLayout>> {
        Some(super::c::layout::parse_struct_fields)
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

    fn blank_error_macros(&self, source_bytes: &[u8]) -> Option<Vec<u8>> {
        blank_c_family_macro_lines(source_bytes)
    }

    fn is_def_capture_allowed(&self, ctx: &DefCaptureGateContext<'_>) -> bool {
        is_c_family_def_allowed(ctx, true)
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
        super::c::is_c_internal_function(root, name_byte, source, true)
    }
}
