use super::{
    ComplexityKinds, DefCaptureGateContext, DialectGroup, LanguageSemantics, SyntaxKinds,
    VisibilityMode, resolve_relative_or_exact, strip_import_quotes,
};
mod arity;
mod bindings;
mod builtins;
mod immutable;
mod members;
mod std_names;
mod types;
mod typed_bindings;

use self::bindings::python_read_position;
use self::immutable::is_py_immutable_binding;
use self::members::{
    has_decorator, is_py_enum_member_target, is_py_self_field_capture, is_staticmethod,
};
use self::std_names::{PYTHON_STD_MEMBERS, PYTHON_STD_PROPERTIES, PYTHON_STD_QUALIFIERS};
use crate::domain::{Lang, SymKind};
use std::collections::HashMap;

pub struct PythonLanguage;
pub static PYTHON_LANG: PythonLanguage = PythonLanguage;

const KINDS: ComplexityKinds = ComplexityKinds {
    paths: &["case_clause"],
    flat: &["elif_clause"],
    nesting: &[
        "if_statement",
        "for_statement",
        "while_statement",
        "except_clause",
        "conditional_expression",
    ],
    nest_only: &["match_statement"],
    else_kinds: &["else_clause"],
    else_bodies: &[],
    jumps: &[
        "break_statement",
        "continue_statement",
        "return_statement",
        "raise_statement",
    ],
    logical_nodes: &[],
    binary_nodes: &["boolean_operator"],
    logical_ops: &["and", "or"],
};

const SYNTAX: SyntaxKinds = SyntaxKinds {
    scope_containers: &["class_definition"],
    member_access: &["attribute"],
    field_access: &["attribute"],
    body_blocks: &["block"],
    body_wrappers: &[
        "function_definition",
        "parenthesized_expression",
        "expression_list",
    ],
    function_defs: &["function_definition"],
    identifiers: &["identifier"],
    type_contexts: &["type_parameter", "generic_type"],
    callable_wrappers: &["parenthesized_expression"],
    binding_patterns: &["tuple_pattern"],
    ..SyntaxKinds::NONE
};

impl LanguageSemantics for PythonLanguage {
    fn local_definition_shadows_name(&self) -> bool {
        true
    }

    fn env_reads(&self) -> &'static [crate::languages::common::flags::EnvRead] {
        use crate::languages::common::flags::EnvRead;
        &[EnvRead::Call("getenv"), EnvRead::Call("environ.get"), EnvRead::Index("environ")]
    }

    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &KINDS
    }

    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }

    fn tag(&self) -> &'static str {
        "py"
    }

    /// Python: a `def` in a class body is reached through an instance or the class, not the module,
    /// and takes the receiver as its first parameter unless it is a `@staticmethod`.
    fn def_takes_self(&self, ctx: &super::DefCaptureGateContext<'_>) -> bool {
        let mut node = ctx.def_node;
        while let Some(current) = node.filter(|n| n.kind() != "block") {
            node = current.parent();
        }
        let in_class = node
            .and_then(|block| block.parent())
            .is_some_and(|owner| owner.kind() == "class_definition");
        in_class && !is_staticmethod(ctx)
    }

    /// A `@property` or `@cached_property` method: reading `x.name` runs it.
    fn def_is_accessor(&self, ctx: &DefCaptureGateContext<'_>) -> bool {
        has_decorator(ctx, |name| {
            name == "property" || name.ends_with("cached_property")
        })
    }

    fn def_is_registered(&self, ctx: &DefCaptureGateContext<'_>) -> bool {
        ctx.def_node
            .and_then(|def| def.parent())
            .is_some_and(|p| {
                p.kind() == "decorated_definition"
                    && super::has_registering_decorator(p, ctx.source_bytes)
            })
    }

    fn member_call_needs_self(&self) -> bool {
        true
    }

    /// Class members are reached through `self.` or a qualifier; a bare `f()` never names one.
    fn bare_calls_skip_type_members(&self) -> bool {
        true
    }

    fn qualifier_must_be_indexed(&self) -> bool {
        true
    }

    fn lint_groups(&self) -> &'static [crate::languages::LintGroup] {
        use crate::languages::LintGroup as G;
        &[G::Parens, G::Unreachable, G::Naming]
    }

    fn lint_return_type<'a>(&self, sig: &'a str, _name: &str) -> Option<&'a str> {
        super::lint_hooks::arrow_return_type(sig)
    }

    fn lint_name_scope<'a>(&self, scope: Option<&'a str>) -> Option<&'a str> {
        scope
    }

    fn is_generated_file_name(&self, file_name: &str) -> bool {
        file_name.ends_with("_pb2.py")
    }
    super::impl_grammar_method!(crate tree_sitter_python::LANGUAGE, "tags.scm");

    fn is_read_capture_allowed(
        &self,
        node: tree_sitter::Node<'_>,
        ancestors: &[tree_sitter::Node<'_>],
        _source: &[u8],
    ) -> bool {
        python_read_position(node, ancestors)
    }

    fn read_bindings<'a>(
        &self,
        root: tree_sitter::Node<'_>,
        source: &'a [u8],
    ) -> super::reads::ReadBindings<'a> {
        typed_bindings::typed_read_bindings(root, source)
    }

    fn implementations(
        &self,
        root: tree_sitter::Node<'_>,
        source: &[u8],
    ) -> Vec<super::Implementation> {
        typed_bindings::class_implementations(root, source)
    }

    fn indexes_implementations(&self) -> bool {
        true
    }

    fn has_class_inheritance(&self) -> bool {
        true
    }

    fn functions_are_type_members(&self) -> bool {
        true
    }

    /// A builtin call (`len(x)`) names the builtin, and `c.m()` on a receiver declared as `C` names
    /// `C.m`; neither edges to a same-named function elsewhere.
    fn call_qualifier(
        &self,
        name_node: tree_sitter::Node<'_>,
        source_bytes: &[u8],
    ) -> Option<String> {
        if builtins::is_builtin_call(name_node, source_bytes) {
            return Some("builtins".to_string());
        }
        types::declared_receiver_class(name_node, source_bytes)
    }

    fn resolve_def_params_and_name(
        &self,
        def_node: Option<tree_sitter::Node<'_>>,
        source_bytes: &[u8],
        _kind: SymKind,
        orig_name: &str,
    ) -> (crate::gate::contract::ParamArity, String) {
        let arity = def_node.map_or(crate::gate::contract::ParamArity::default(), |n| {
            arity::python_def_arity(n, self.syntax_kinds(), source_bytes)
        });
        (arity, orig_name.to_string())
    }

    fn var_binding(&self, ctx: &DefCaptureGateContext<'_>) -> crate::domain::Binding {
        if is_py_immutable_binding(ctx) {
            crate::domain::Binding::Frozen
        } else {
            crate::domain::Binding::Mutable
        }
    }

    fn extensions(&self) -> &'static [&'static str] {
        &["py", "pyi"]
    }

    fn test_run_command(
        &self,
        path: &str,
        source: &str,
        _manifest: Option<&str>,
    ) -> Option<String> {
        (source.contains("__name__ == \"__main__\"") || source.contains("__name__ == '__main__'"))
            .then(|| format!("python3 {path}"))
    }

    fn display_color(&self) -> &'static str {
        "#3572a5"
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Isolated(Lang::Python)
    }

    fn uses_hash_comments(&self) -> bool {
        true
    }

    fn visibility_mode(&self) -> VisibilityMode {
        VisibilityMode::ByNameConvention
    }

    fn is_symbol_name_public(&self, sym_name: &str) -> bool {
        !sym_name.starts_with('_') && !sym_name.starts_with('#')
    }

    fn is_def_capture_allowed(&self, ctx: &DefCaptureGateContext<'_>) -> bool {
        if ctx.kind == SymKind::Field {
            return is_py_self_field_capture(ctx.name_node, ctx.def_node, ctx.source_bytes);
        }
        if ctx.sub == Some("enummember") {
            return is_py_enum_member_target(ctx.name_node, ctx.source_bytes);
        }
        true
    }

    fn std_member_names(&self) -> &'static [&'static str] {
        PYTHON_STD_MEMBERS
    }

    fn std_property_names(&self) -> &'static [&'static str] {
        PYTHON_STD_PROPERTIES
    }

    fn std_qualifier_names(&self) -> &'static [&'static str] {
        PYTHON_STD_QUALIFIERS
    }

    fn import_binding<'a>(&self, raw_target: &'a str) -> Option<(&'a str, &'a str)> {
        let clean = strip_import_quotes(raw_target);
        let (module, bound) = clean
            .split_once(" as ")
            .map_or((clean, clean), |(m, a)| (m.trim(), a.trim()));
        (!module.is_empty()).then_some((module, bound))
    }

    fn resolve_import_target(
        &self,
        importer_path: &str,
        raw_target: &str,
        file_index: &HashMap<&str, u32>,
    ) -> Option<u32> {
        let (module, _) = self.import_binding(raw_target)?;
        let rel_mod = module.trim_start_matches('.').replace('.', "/");
        if rel_mod.is_empty() {
            return None;
        }
        resolve_relative_or_exact(
            importer_path,
            &rel_mod,
            file_index,
            &[".py", "/__init__.py"],
        )
    }
}
