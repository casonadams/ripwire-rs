use super::c::{
    C_FAMILY_KEYWORDS, C_LIBRARY_FUNCTIONS, CPP_STD_MEMBERS, CPP_STD_QUALIFIERS,
    blank_c_family_macro_lines, type_aliases,
    c_family_declaration_ties, is_c_family_def_allowed, refine_c_header_dialect,
    resolve_c_family_def_body_and_end, resolve_c_family_include,
};
use super::{
    ComplexityKinds, DefCaptureGateContext, DialectGroup, ForeignExport, LanguageSemantics,
    SyntaxKinds,
};
use crate::domain::Lang;
use std::collections::HashMap;

pub struct CppLanguage;
pub static CPP_LANG: CppLanguage = CppLanguage;

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

/// Python module macros whose `.def("name", &fn)` calls expose C++ definitions to Python.
const PYTHON_MODULE_MACROS: &[&[u8]] = &[b"PYBIND11_MODULE", b"BOOST_PYTHON_MODULE"];

/// Core members of the standard smart pointers, `std::optional` and `swap`. On a smart pointer such a call is the
/// wrapper's own (`formatter_.reset()` on a `std::unique_ptr<formatter>` is `unique_ptr::reset`), and on an untyped
/// receiver it names the standard member, never an in-tree class's.
const CPP_CORE_MEMBERS: &[&str] = &[
    "emplace",
    "expired",
    "get",
    "has_value",
    "lock",
    "owner_before",
    "release",
    "reset",
    "swap",
    "use_count",
    "value",
    "value_or",
];

fn string_literal_text<'a>(node: tree_sitter::Node<'_>, source: &'a [u8]) -> Option<&'a str> {
    (node.kind() == "string_literal")
        .then(|| node.utf8_text(source).ok())??
        .strip_prefix('"')?
        .strip_suffix('"')
}

/// `.def("exported", &target)` / `.def("exported", &Scope::target)` as a Python export.
fn python_def_export(call: tree_sitter::Node<'_>, source: &[u8]) -> Option<ForeignExport> {
    let field = call
        .child_by_field_name("function")?
        .child_by_field_name("field")?;
    if field.utf8_text(source).ok()? != "def" {
        return None;
    }
    let args = call.child_by_field_name("arguments")?;
    let exported = string_literal_text(args.named_child(0)?, source)?;
    let pointer = args.named_child(1)?;
    if pointer.kind() != "pointer_expression" {
        return None;
    }
    let target = pointer.child_by_field_name("argument")?;
    let (target_scope, target_name) = match target.kind() {
        "identifier" => (None, target),
        "qualified_identifier" => (
            target.child_by_field_name("scope"),
            target.child_by_field_name("name")?,
        ),
        _ => return None,
    };
    Some(ForeignExport {
        exported: exported.to_string(),
        target: target_name.utf8_text(source).ok()?.to_string(),
        target_scope: target_scope
            .and_then(|s| s.utf8_text(source).ok())
            .map(str::to_string),
        caller: Lang::Python,
    })
}

fn python_module_exports(root: tree_sitter::Node<'_>, source: &[u8]) -> Vec<ForeignExport> {
    let mut exports = Vec::new();
    let mut cursor = root.walk();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression"
            && let Some(export) = python_def_export(node, source)
        {
            exports.push(export);
        }
        stack.extend(node.named_children(&mut cursor));
    }
    exports
}

pub(super) const SYNTAX: SyntaxKinds = SyntaxKinds {
    scope_containers: &[
        "class_specifier",
        "struct_specifier",
        "namespace_definition",
    ],
    scoped_paths: &["qualified_identifier"],
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
    local_bindings: &["for_range_loop"],
    identifiers: &["identifier"],
    type_contexts: &["template_argument_list", "template_parameter_list"],
    callables: &["function_declarator"],
    callable_wrappers: &["parenthesized_expression"],
    signature_holders: &["declaration"],
    binding_patterns: &[
        "pointer_declarator",
        "reference_declarator",
        "array_declarator",
        "parenthesized_declarator",
    ],
    ..SyntaxKinds::NONE
};

const TYPED: super::typed::TypedSyntax = super::typed::TypedSyntax {
    classes: &["class_specifier", "struct_specifier"],
    supertype_clauses: &["base_class_clause"],
    params: &["parameter_declaration", "optional_parameter_declaration"],
    functions: &["function_definition", "lambda_expression"],
    locals: &["declaration"],
    fields: &["field_declaration"],
    declarators: &[],
    blocks: &["compound_statement"],
    constructions: &["new_expression"],
    inferred: &["auto"],
    assignments: &[],
    calls: &[("call_expression", "function")],
    return_type_field: "type",
    superclass: None,
    declaration_name: Some("declarator"),
    class_params: &[],
    transparent_wrappers: &["std::unique_ptr", "std::shared_ptr", "std::weak_ptr", "std::optional"],
};

/// Scope of a member inside an explicit specialization keeps its template arguments
/// (`template <> struct Box<int>` scopes `Box<int>`), so it stays distinct from the primary.
pub(super) fn specialization_scope(parent: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    if !matches!(parent.kind(), "class_specifier" | "struct_specifier") {
        return None;
    }
    let name = parent.child_by_field_name("name")?;
    (name.kind() == "template_type")
        .then(|| name.utf8_text(source).ok())??
        .split_whitespace()
        .collect::<String>()
        .into()
}

pub(super) fn is_template_specialization(spec: &str, primary: &str) -> bool {
    spec.strip_prefix(primary)
        .is_some_and(|args| args.starts_with('<'))
}

impl LanguageSemantics for CppLanguage {
    crate::languages::c::flags::c_family_hooks!();

    fn flag_manifests(&self) -> &'static [&'static str] {
        &["CMakeLists.txt", "cmake/options.cmake"]
    }

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
    fn is_specialization_of(&self, spec: &str, primary: &str) -> bool {
        is_template_specialization(spec, primary)
    }
    fn extract_custom_scope(
        &self,
        parent: tree_sitter::Node<'_>,
        _name_node: tree_sitter::Node<'_>,
        source_bytes: &[u8],
    ) -> Option<String> {
        specialization_scope(parent, source_bytes)
    }
    fn foreign_exports(&self, root: tree_sitter::Node<'_>, source: &[u8]) -> Vec<ForeignExport> {
        let declares_module = PYTHON_MODULE_MACROS
            .iter()
            .any(|m| source.windows(m.len()).any(|w| w == *m));
        if declares_module {
            python_module_exports(root, source)
        } else {
            Vec::new()
        }
    }
    super::impl_typed_class_hooks!(TYPED);
    fn receiver_text<'a>(&self, text: &'a str) -> std::borrow::Cow<'a, str> {
        if text.contains("->") {
            std::borrow::Cow::Owned(text.replace("->", "."))
        } else {
            std::borrow::Cow::Borrowed(text)
        }
    }
    fn tag(&self) -> &'static str {
        "cpp"
    }

    fn lint_groups(&self) -> &'static [crate::languages::LintGroup] {
        super::lint::C_FAMILY_LINT_GROUPS
    }
    super::impl_grammar_method!(crate tree_sitter_cpp::LANGUAGE, "tags.scm");

    fn lint_return_type<'a>(&self, sig: &'a str, name: &str) -> Option<&'a str> {
        super::lint_hooks::prefix_return_type(sig, name)
    }

    fn type_declaration_keywords(&self) -> &'static [&'static str] {
        &["typedef", "enum", "using"]
    }

    fn supports_cache_lint(&self) -> bool {
        true
    }

    fn untyped_member_declines_caller_class(&self) -> bool {
        true
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["h", "cpp", "cc", "cxx", "hpp", "hxx", "hh"]
    }

    fn display_color(&self) -> &'static str {
        "#f34b7d"
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::CFamily
    }

    fn layout_model(&self) -> Option<fn(&str) -> Vec<crate::metrics::FieldLayout>> {
        Some(super::c::layout::parse_struct_fields)
    }

    fn refine_dialect(
        &self,
        base_lang: crate::domain::Lang,
        rel_path: &str,
        source_bytes: &[u8],
    ) -> crate::domain::Lang {
        refine_c_header_dialect(base_lang, rel_path, source_bytes)
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

    fn core_member_names(&self) -> &'static [&'static str] {
        CPP_CORE_MEMBERS
    }

    fn library_function_names(&self) -> &'static [&'static str] {
        C_LIBRARY_FUNCTIONS
    }

    fn cross_file_field_types(&self) -> bool {
        true
    }

    fn type_aliases(&self, root: tree_sitter::Node<'_>, source: &[u8]) -> Vec<(String, String)> {
        type_aliases(root, source)
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
