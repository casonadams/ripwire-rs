use crate::domain::Binding;
use super::{ComplexityKinds, DefCaptureGateContext, DialectGroup, LanguageSemantics, SyntaxKinds};
use crate::languages::strip_import_quotes;
use std::collections::HashMap;

mod access;
mod binding;
mod defs;
mod imports;
mod locals;
mod names;
mod next;
mod scalar;
mod test_runner;
mod tsconfig;
mod types;

pub(super) use self::access::{is_write_target_root, javascript_read_position};
pub(super) use self::binding::{javascript_binding_scope, javascript_binding_scope_in};
pub(super) use self::defs::{
    is_container_const, is_jsx_intrinsic_tag_call, is_std_member_call_value,
    js_definition_span_start, js_var_binding,
};
use self::defs::{is_cjs_export_target, is_member_assigned_function_name, is_prototype_member_target};
use self::imports::js_import_refs;
pub(super) use self::imports::{default_export_names, dynamic_imports};
pub(super) use self::imports::resolve_web_import_target;
pub(super) use self::locals::local_binding_names;
pub(super) use self::next::is_next_convention_file;
pub(super) use self::names::{JS_STD_MEMBERS, JS_STD_QUALIFIERS};
pub(super) use self::test_runner::{is_assertion_script, js_test_partner_subject, js_test_run_command};
pub(super) use self::tsconfig::{TS_CONFIG_MANIFESTS, parse_tsconfig_paths};

pub struct JavaScriptLanguage;
pub static JAVASCRIPT_LANG: JavaScriptLanguage = JavaScriptLanguage;

pub(super) const JS_KINDS: ComplexityKinds = ComplexityKinds {
    paths: &["switch_case"],
    flat: &[],
    nesting: &[
        "if_statement",
        "for_statement",
        "for_in_statement",
        "while_statement",
        "do_statement",
        "catch_clause",
        "ternary_expression",
    ],
    nest_only: &["switch_statement"],
    else_kinds: &["else_clause"],
    else_bodies: &[],
    jumps: &[
        "break_statement",
        "continue_statement",
        "return_statement",
        "throw_statement",
    ],
    logical_nodes: &[],
    binary_nodes: &["binary_expression"],
    logical_ops: &["&&", "||", "??"],
};

const SYNTAX: SyntaxKinds = SyntaxKinds {
    scope_containers: &["class", "class_declaration"],
    member_access: &["member_expression"],
    field_access: &["member_expression"],
    body_blocks: &["statement_block", "class_body"],
    body_wrappers: &[
        "lexical_declaration",
        "variable_declaration",
        "variable_declarator",
        "field_definition",
        "export_statement",
        "arrow_function",
        "function_expression",
        "generator_function",
        "parenthesized_expression",
    ],
    function_defs: &["function_declaration", "method_definition"],
    local_bindings: &["for_in_statement"],
    non_local_declarations: &["function_declaration", "class_declaration"],
    identifiers: &["identifier", "shorthand_property_identifier_pattern"],
    callables: &["arrow_function", "function_expression"],
    callable_wrappers: &["variable_declarator", "parenthesized_expression"],
    binding_patterns: &[
        "object_pattern",
        "array_pattern",
        "pair_pattern",
        "rest_pattern",
        "assignment_pattern",
    ],
    ..SyntaxKinds::NONE
};

impl LanguageSemantics for JavaScriptLanguage {
    fn local_definition_shadows_name(&self) -> bool {
        true
    }

    fn dynamic_imports(
        &self,
        root: tree_sitter::Node<'_>,
        source: &[u8],
    ) -> Vec<(String, Vec<String>)> {
        dynamic_imports(root, source)
    }

    fn default_export_names(&self, root: tree_sitter::Node<'_>, source: &[u8]) -> Vec<String> {
        default_export_names(root, source)
    }

    fn local_bindings(&self, root: tree_sitter::Node<'_>, source: &[u8]) -> Vec<(String, u32, u32)> {
        local_binding_names(root, source)
    }

    fn is_entrypoint_symbol(
        &self,
        sym_name: &str,
        scope: Option<&str>,
        kind: crate::domain::SymKind,
        rel_path: &str,
    ) -> bool {
        kind.is_callable()
            && !super::is_non_entrypoint_path(rel_path)
            && ((sym_name == "main" && scope.is_none()) || is_next_convention_file(rel_path))
    }

    /// Class members are reached through `this.` or a qualifier; a bare `f()` never names one.
    fn bare_calls_skip_type_members(&self) -> bool {
        true
    }

    fn env_reads(&self) -> &'static [crate::languages::common::flags::EnvRead] {
        &[crate::languages::common::flags::EnvRead::Member("process.env")]
    }

    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &JS_KINDS
    }

    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }

    fn tag(&self) -> &'static str {
        "js"
    }

    fn lint_groups(&self) -> &'static [crate::languages::LintGroup] {
        super::lint::SCRIPT_LINT_GROUPS
    }

    fn is_generated_file_name(&self, file_name: &str) -> bool {
        file_name.ends_with(".min.js")
    }
    super::impl_grammar_method!(crate tree_sitter_javascript::LANGUAGE, "tags.scm");

    fn indexes_implementations(&self) -> bool {
        true
    }

    fn implementations(
        &self,
        root: tree_sitter::Node<'_>,
        source: &[u8],
    ) -> Vec<crate::languages::Implementation> {
        types::class_implementations(root, source)
    }

    fn has_class_inheritance(&self) -> bool {
        true
    }

    fn is_read_capture_allowed(
        &self,
        node: tree_sitter::Node<'_>,
        ancestors: &[tree_sitter::Node<'_>],
        source: &[u8],
    ) -> bool {
        javascript_read_position(node, ancestors, source)
    }

    fn is_write_capture_allowed(
        &self,
        node: tree_sitter::Node<'_>,
        ancestors: &[tree_sitter::Node<'_>],
        source: &[u8],
    ) -> bool {
        is_write_target_root(node, ancestors, source)
    }

    fn read_bindings<'a>(
        &self,
        root: tree_sitter::Node<'_>,
        source: &'a [u8],
    ) -> super::reads::ReadBindings<'a> {
        types::typed_read_bindings(root, source)
    }

    /// A receiver typed by an object type or a plain name the index does not know keeps name resolution.
    fn unindexed_receiver_type_uses_names(&self, _receiver_type: &str) -> bool {
        true
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["js", "jsx", "mjs", "cjs"]
    }

    fn test_runner_manifest(&self) -> Option<&'static str> {
        Some("package.json")
    }

    fn test_run_command(&self, path: &str, source: &str, manifest: Option<&str>) -> Option<String> {
        js_test_run_command(path, source, manifest, true)
    }

    fn display_color(&self) -> &'static str {
        "#f1e05a"
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Web
    }

    fn is_def_capture_allowed(&self, ctx: &DefCaptureGateContext<'_>) -> bool {
        match ctx.sub {
            Some("constant") => super::is_screaming_snake(ctx.name) || is_container_const(ctx),
            Some("cjsexport") => ctx
                .name_node
                .is_some_and(|nn| is_cjs_export_target(nn, ctx.source_bytes)),
            Some("protomethod") => ctx
                .name_node
                .is_some_and(|nn| is_prototype_member_target(nn, ctx.source_bytes)),
            None => !is_member_assigned_function_name(ctx),
            _ => true,
        }
    }

    fn var_binding(&self, ctx: &DefCaptureGateContext<'_>) -> Binding {
        js_var_binding(ctx)
    }

    fn resolve_def_span_start(
        &self,
        def_node: tree_sitter::Node<'_>,
        source_bytes: &[u8],
    ) -> (usize, usize) {
        js_definition_span_start(def_node, source_bytes)
    }

    fn test_partner_subject<'a>(&self, file_stem: &'a str, in_test_dir: bool) -> Option<&'a str> {
        js_test_partner_subject(file_stem, in_test_dir)
    }

    fn is_assertion_script(&self, path: &str) -> bool {
        is_assertion_script(path)
    }

    fn member_call_targets_callables_only(&self) -> bool {
        true
    }

    fn should_ignore_call(
        &self,
        name_node: Option<tree_sitter::Node<'_>>,
        _ancestors: &[tree_sitter::Node<'_>],
        name: &str,
    ) -> bool {
        is_jsx_intrinsic_tag_call(name_node, name)
    }

    fn std_member_names(&self) -> &'static [&'static str] {
        JS_STD_MEMBERS
    }

    fn std_qualifier_names(&self) -> &'static [&'static str] {
        JS_STD_QUALIFIERS
    }

    fn import_alias_manifests(&self) -> &'static [&'static str] {
        TS_CONFIG_MANIFESTS
    }

    fn parse_import_aliases(&self, content: &str) -> Vec<(String, String)> {
        parse_tsconfig_paths(content)
    }

    fn resolve_import_target(
        &self,
        importer_path: &str,
        raw_target: &str,
        file_index: &HashMap<&str, u32>,
    ) -> Option<u32> {
        resolve_web_import_target(importer_path, raw_target, file_index)
    }

    fn import_binding<'a>(&self, raw_target: &'a str) -> Option<(&'a str, &'a str)> {
        let (module, bound) = raw_target.split_once(" as ")?;
        Some((strip_import_quotes(module), bound.trim()))
    }

    fn import_refs(
        &self,
        root: tree_sitter::Node<'_>,
        source: &[u8],
        refs: &mut Vec<crate::ingest::RawRef>,
    ) {
        js_import_refs(root, source, refs);
    }
}
