mod binding_types;
mod callables;
mod implementations;
mod linkage;
mod names;
mod positions;
mod read_bindings;
mod resolve;
mod scope;
mod syntax;
mod types;
mod use_tree;

use super::{
    ComplexityKinds, DefCaptureGateContext, DialectGroup, LanguageSemantics, SyntaxKinds,
    VisibilityMode, strip_import_quotes,
};
use crate::domain::{Lang, SymKind};
use implementations::collect_rust_implementations;
use names::{
    RUST_BUILTIN_MACROS, RUST_ITER_STR_MEMBERS, RUST_STD_MEMBERS, RUST_STD_QUALIFIERS,
    RUST_TREE_SITTER_MEMBERS,
};
use positions::{
    is_function_name, is_macro_argument_call, macro_path_qualifier, rust_read_position,
};
use read_bindings::collect_rust_read_bindings;
use resolve::resolve_rust_use_target;
use std::collections::HashMap;
use syntax::{KINDS, SYNTAX};
use types::nominal_name;
use use_tree::use_tree_bindings;

pub struct RustLanguage;
pub static RUST_LANG: RustLanguage = RustLanguage;

impl LanguageSemantics for RustLanguage {
    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &KINDS
    }

    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }

    fn extract_custom_scope(
        &self,
        parent: tree_sitter::Node<'_>,
        _name_node: tree_sitter::Node<'_>,
        source_bytes: &[u8],
    ) -> Option<String> {
        if parent.kind() != "impl_item" {
            return None;
        }
        let ty = parent.child_by_field_name("type")?;
        let text = ty.utf8_text(source_bytes).ok()?;
        let name = text.split('<').next().unwrap_or(text).trim();
        (!name.is_empty()).then(|| name.to_string())
    }

    fn bare_calls_skip_type_members(&self) -> bool {
        true
    }

    fn interface_calls_keep_declaration(&self) -> bool {
        true
    }

    fn tag(&self) -> &'static str {
        "rs"
    }

    fn lint_groups(&self) -> &'static [crate::languages::LintGroup] {
        use crate::languages::LintGroup as G;
        &[G::Insecure, G::Parens, G::SelfAssign, G::Naming]
    }

    fn lint_return_type<'a>(&self, sig: &'a str, _name: &str) -> Option<&'a str> {
        super::lint_hooks::arrow_return_type(sig)
    }

    fn lint_name_scope<'a>(&self, scope: Option<&'a str>) -> Option<&'a str> {
        scope
    }

    fn type_declaration_keywords(&self) -> &'static [&'static str] {
        &["type"]
    }

    fn is_test_module_stem(&self, file_stem: &str) -> bool {
        let lower = file_stem.to_ascii_lowercase();
        matches!(lower.as_str(), "tests" | "test") || lower.ends_with("_tests")
    }

    /// Outside `tests/` and `benches/` a `test_` basename is a module (`test_gate.rs`); the `_test` and
    /// `_spec` suffixes stay partner names.
    fn test_partner_subject<'a>(&self, file_stem: &'a str, in_test_dir: bool) -> Option<&'a str> {
        super::common::util::universal_test_partner_subject(file_stem, in_test_dir)
            .filter(|_| in_test_dir || !file_stem.starts_with("test_"))
    }

    super::impl_grammar_method!(crate tree_sitter_rust::LANGUAGE, "tags.scm");

    fn is_read_capture_allowed(
        &self,
        node: tree_sitter::Node<'_>,
        ancestors: &[tree_sitter::Node<'_>],
        _source: &[u8],
    ) -> bool {
        rust_read_position(node, ancestors)
    }

    fn callable_bindings(
        &self,
        root: tree_sitter::Node<'_>,
        source: &[u8],
    ) -> Vec<super::CallableBinding> {
        callables::collect(root, source)
    }

    fn is_std_or_builtin_qualifier(&self, qualifier: &str) -> bool {
        let root = qualifier.split("::").next().unwrap_or(qualifier);
        matches!(root, "std" | "core" | "alloc") || RUST_STD_QUALIFIERS.contains(&qualifier)
    }

    fn implementations(
        &self,
        root: tree_sitter::Node<'_>,
        source: &[u8],
    ) -> Vec<super::Implementation> {
        collect_rust_implementations(root, source)
    }

    fn read_bindings<'a>(
        &self,
        root: tree_sitter::Node<'_>,
        source: &'a [u8],
    ) -> super::reads::ReadBindings<'a> {
        collect_rust_read_bindings(root, source)
    }

    fn has_lifetime_tick_syntax(&self) -> bool {
        true
    }

    fn display_color(&self) -> &'static str {
        "#dea584"
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Isolated(Lang::Rust)
    }

    fn has_trait_impls(&self) -> bool {
        true
    }

    fn self_receivers(&self) -> &'static [&'static str] {
        &["Self"]
    }

    fn keeps_path_qualifier(&self) -> bool {
        true
    }

    /// `crate::crawler::CrawlerOptions::default()`: the qualifier is the type segment (`CrawlerOptions`),
    /// not the whole module path, so the call binds to that type's own method rather than being
    /// rejected as a std-name member. `Self` and non-type segments keep the default path handling.
    fn call_qualifier(
        &self,
        name_node: tree_sitter::Node<'_>,
        source_bytes: &[u8],
    ) -> Option<String> {
        if name_node.parent().is_some_and(|p| p.kind() == "token_tree") {
            return macro_path_qualifier(name_node, source_bytes);
        }
        let path = name_node.parent().filter(|p| p.kind() == "scoped_identifier")?;
        let path = path.child_by_field_name("path")?;
        if path.kind() != "scoped_identifier" {
            return None;
        }
        let segment = path.child_by_field_name("name")?.utf8_text(source_bytes).ok()?;
        let is_type = segment.bytes().next().is_some_and(|b| b.is_ascii_uppercase())
            && segment != "Self"
            && segment.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_');
        is_type.then(|| segment.to_string())
    }

    fn def_takes_self(&self, ctx: &DefCaptureGateContext<'_>) -> bool {
        ctx.def_node
            .and_then(|def| def.child_by_field_name("parameters"))
            .and_then(|params| params.named_child(0))
            .is_some_and(|first| first.kind() == "self_parameter")
    }

    fn member_call_needs_self(&self) -> bool {
        true
    }

    /// `build(..).m()` and `module::build(..).m()`: the receiver is typed by the free function's declared
    /// return type. A path whose owner segment is a type (`Type::build()`) is an associated function, not
    /// the free function of that name, so it is left untyped.
    fn call_receiver_name<'a>(
        &self,
        receiver: tree_sitter::Node<'_>,
        source: &'a [u8],
    ) -> Option<&'a str> {
        let callee = receiver
            .child_by_field_name("function")
            .filter(|_| receiver.kind() == "call_expression")?;
        match callee.kind() {
            "identifier" => callee.utf8_text(source).ok(),
            "field_expression" => {
                let receiver = callee.child_by_field_name("value")?;
                (receiver.utf8_text(source).ok()? == "self")
                    .then(|| callee.utf8_text(source).ok())?
            }
            "scoped_identifier" => {
                let owner = callee.child_by_field_name("path")?.utf8_text(source).ok()?;
                let segment = owner.rsplit("::").next()?;
                let is_module = segment
                    .bytes()
                    .next()
                    .is_some_and(|b| b.is_ascii_lowercase() || b == b'_');
                is_module
                    .then(|| callee.child_by_field_name("name")?.utf8_text(source).ok())?
            }
            _ => None,
        }
    }

    fn crate_root_suffixes(&self) -> &'static [&'static str] {
        &["src/lib.rs", "src/main.rs"]
    }

    fn type_mentions_are_edges(&self) -> bool {
        true
    }

    fn is_callable_field_access(&self, name_node: tree_sitter::Node<'_>) -> bool {
        let kinds: Vec<&str> = std::iter::successors(name_node.parent(), |n| n.parent())
            .take(3)
            .map(|n| n.kind())
            .collect();
        kinds
            == [
                "field_expression",
                "parenthesized_expression",
                "call_expression",
            ]
    }

    fn visibility_mode(&self) -> VisibilityMode {
        VisibilityMode::ExplicitModifier
    }

    fn should_ignore_call(
        &self,
        name_node: Option<tree_sitter::Node<'_>>,
        ancestors: &[tree_sitter::Node<'_>],
        name: &str,
    ) -> bool {
        let Some(node) = name_node else {
            return false;
        };
        match ancestors.first().map(|p| p.kind()) {
            Some("macro_invocation") => RUST_BUILTIN_MACROS.contains(&name),
            Some("token_tree") => {
                !(is_macro_argument_call(node, ancestors) && is_function_name(name))
            }
            _ => false,
        }
    }

    fn is_macro_argument(
        &self,
        node: tree_sitter::Node<'_>,
        ancestors: &[tree_sitter::Node<'_>],
    ) -> bool {
        node.kind() == "token_tree" || ancestors.iter().any(|n| n.kind() == "token_tree")
    }

    fn struct_update_base_type(
        &self,
        name_node: tree_sitter::Node<'_>,
        source: &[u8],
    ) -> Option<String> {
        let path = name_node
            .parent()
            .filter(|p| p.kind() == "scoped_identifier")?;
        let call = path.parent().filter(|c| c.kind() == "call_expression")?;
        let base = call
            .parent()
            .filter(|b| b.kind() == "base_field_initializer")?;
        let literal = base
            .parent()?
            .parent()
            .filter(|s| s.kind() == "struct_expression")?;
        if path.child_by_field_name("path")?.utf8_text(source).ok()? != "Default" {
            return None;
        }
        let text = literal.child_by_field_name("name")?.utf8_text(source).ok()?;
        let leaf = text.split('<').next()?.rsplit("::").next()?.trim();
        (!leaf.is_empty() && leaf != "Self").then(|| leaf.to_string())
    }

    fn default_constructor_name(&self) -> Option<&'static str> {
        Some("default")
    }

    fn std_member_names(&self) -> &'static [&'static str] {
        RUST_STD_MEMBERS
    }

    fn is_std_or_builtin_member(&self, name: &str) -> bool {
        RUST_STD_MEMBERS.contains(&name)
            || RUST_ITER_STR_MEMBERS.contains(&name)
            || RUST_TREE_SITTER_MEMBERS.contains(&name)
    }

    fn std_qualifier_names(&self) -> &'static [&'static str] {
        RUST_STD_QUALIFIERS
    }

    fn symbol_implied_import(&self, kind: SymKind, name: &str) -> Option<String> {
        (kind == SymKind::Section).then(|| format!("self::{name}"))
    }
    fn import_module_bindings(&self, raw_target: &str) -> Vec<(String, String)> {
        use_tree_bindings(strip_import_quotes(raw_target))
    }
    fn import_reexports(&self) -> bool {
        true
    }
    fn module_declarations_are_anchors(&self) -> bool {
        true
    }
    fn resolve_import_target(
        &self,
        importer_path: &str,
        raw_target: &str,
        file_index: &HashMap<&str, u32>,
    ) -> Option<u32> {
        resolve_rust_use_target(importer_path, raw_target, file_index)
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
        linkage::is_rust_internal_function(root, name_byte, source)
    }

    fn var_binding(&self, ctx: &DefCaptureGateContext<'_>) -> crate::domain::Binding {
        if ctx
            .def_node
            .is_some_and(|def| is_rust_immutable_binding(def, ctx.source_bytes))
        {
            crate::domain::Binding::Frozen
        } else {
            crate::domain::Binding::Mutable
        }
    }
}

const RUST_INTERIOR_MUTABLE_TYPES: &[&str] = &[
    "Cell",
    "RefCell",
    "UnsafeCell",
    "OnceCell",
    "OnceLock",
    "Mutex",
    "RwLock",
    "Once",
];

/// A `const` is never state; a `static` is state only when it is `mut` or its type has interior mutability.
fn is_rust_immutable_binding(def: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    match def.kind() {
        "const_item" => true,
        "static_item" => {
            let mut cursor = def.walk();
            let is_mut = def
                .children(&mut cursor)
                .any(|child| child.kind() == "mutable_specifier");
            let interior = def
                .child_by_field_name("type")
                .and_then(|ty| nominal_name(source, ty))
                .map(|path| path.rsplit("::").next().unwrap_or(path))
                .is_some_and(|name| {
                    RUST_INTERIOR_MUTABLE_TYPES.contains(&name) || name.starts_with("Atomic")
                });
            !is_mut && !interior
        }
        _ => false,
    }
}
