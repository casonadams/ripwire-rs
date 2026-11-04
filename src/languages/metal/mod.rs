use super::c::{
    C_FAMILY_KEYWORDS, CPP_STD_MEMBERS, CPP_STD_QUALIFIERS, is_c_family_def_allowed,
    resolve_c_family_def_body_and_end, resolve_c_family_include,
};
use super::{ComplexityKinds, DefCaptureGateContext, DialectGroup, LanguageSemantics, SyntaxKinds};
use std::collections::HashMap;

pub struct MetalLanguage;
pub static METAL_LANG: MetalLanguage = MetalLanguage;

/// Metal qualifiers the C++ grammar cannot parse: function kinds (`kernel`, `vertex`, `fragment`) and address
/// spaces (`device`, `constant`, `thread`, `threadgroup`). Reserved words in Metal, so no identifier carries them.
const METAL_QUALIFIERS: &[&[u8]] = &[
    b"kernel",
    b"vertex",
    b"fragment",
    b"device",
    b"constant",
    b"thread",
    b"threadgroup",
    b"threadgroup_imageblock",
    b"ray_data",
    b"object_data",
];

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// The source with every Metal qualifier word blanked to spaces, so byte offsets and names are unchanged.
/// Preprocessor lines are left alone: an include path such as `"device.h"` is not a qualifier.
fn blank_metal_qualifiers(source: &[u8]) -> Option<Vec<u8>> {
    let mut out = source.to_vec();
    let mut changed = false;
    let mut at_line_start = true;
    let mut i = 0;
    while i < source.len() {
        let b = source[i];
        if b == b'\n' {
            at_line_start = true;
            i += 1;
        } else if at_line_start && b == b'#' {
            i = source[i..]
                .iter()
                .position(|&c| c == b'\n')
                .map_or(source.len(), |p| i + p);
        } else if b.is_ascii_whitespace() {
            i += 1;
        } else if is_ident_byte(b) {
            at_line_start = false;
            let end = source[i..]
                .iter()
                .position(|&c| !is_ident_byte(c))
                .map_or(source.len(), |p| i + p);
            if METAL_QUALIFIERS.contains(&&source[i..end]) {
                out[i..end].fill(b' ');
                changed = true;
            }
            i = end;
        } else {
            at_line_start = false;
            i += 1;
        }
    }
    changed.then_some(out)
}

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

impl LanguageSemantics for MetalLanguage {
    crate::languages::c::flags::c_family_hooks!();

    /// Metal shaders use qualifiers the C++ grammar rejects; the parser retries on this blanked source.
    fn blank_error_macros(&self, source_bytes: &[u8]) -> Option<Vec<u8>> {
        blank_metal_qualifiers(source_bytes)
    }

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
        "metal"
    }
    super::impl_grammar_method!(crate tree_sitter_cpp::LANGUAGE, "../cpp/tags.scm");

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

    fn display_color(&self) -> &'static str {
        "#3a87ad"
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::CFamily
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
