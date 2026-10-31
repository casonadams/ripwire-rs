mod types;

use super::{
    ComplexityKinds, DefCaptureGateContext, DialectGroup, LanguageSemantics, SyntaxKinds,
    VisibilityMode,
};
use crate::domain::{Lang, SymKind};
use std::collections::HashMap;

pub struct DartLanguage;
pub static DART_LANG: DartLanguage = DartLanguage;

const DART_STD_MEMBERS: &[&str] = &[
    "add",
    "addAll",
    "remove",
    "clear",
    "contains",
    "containsKey",
    "map",
    "where",
    "expand",
    "forEach",
    "fold",
    "reduce",
    "toList",
    "toSet",
    "join",
    "split",
    "trim",
    "substring",
    "replaceAll",
    "startsWith",
    "endsWith",
    "toLowerCase",
    "toUpperCase",
    "toString",
    "compareTo",
    "then",
    "catchError",
    "whenComplete",
    "isAfter",
    "isBefore",
    "isAtSameMomentAs",
    "difference",
    "toUtc",
    "toLocal",
    "toIso8601String",
    "firstWhere",
    "singleWhere",
    "any",
    "every",
    "take",
    "skip",
    "cast",
    "asMap",
    "putIfAbsent",
    "update",
    "removeWhere",
    "wait",
    "value",
    "error",
    "delayed",
    "decode",
    "encode",
];

const DART_STD_QUALIFIERS: &[&str] = &[
    "Future",
    "Stream",
    "List",
    "Map",
    "Set",
    "Iterable",
    "String",
    "int",
    "double",
    "num",
    "bool",
    "DateTime",
    "Duration",
    "Uri",
    "RegExp",
    "JsonCodec",
    "json",
    "utf8",
    "math",
    "Platform",
    "File",
    "Directory",
];

fn dart_following_body(node: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let mut sib = node.next_sibling();
    while let Some(s) = sib {
        if s.kind() == "function_body" {
            return Some(s);
        }
        if s.is_named() {
            break;
        }
        sib = s.next_sibling();
    }
    None
}

fn resolve_dart_package_import(
    importer_path: &str,
    pkg_spec: &str,
    file_index: &HashMap<&str, u32>,
) -> Option<u32> {
    let (pkg, subpath) = pkg_spec.split_once('/')?;
    if let Some((top_dir, _)) = importer_path.split_once('/') {
        let ws_cand = format!("{top_dir}/{pkg}/lib/{subpath}");
        if let Some(id) = super::probe_with_suffixes(&ws_cand, file_index, &[".dart"]) {
            return Some(id);
        }
    }
    for cand in [
        format!("packages/{pkg}/lib/{subpath}"),
        format!("{pkg}/lib/{subpath}"),
        format!("lib/{subpath}"),
    ] {
        if let Some(id) = super::probe_with_suffixes(&cand, file_index, &[".dart"]) {
            return Some(id);
        }
    }
    None
}

/// The leading identifier of a pure selector chain (`x.f(a)[0]`), else the whole expression.
fn dart_selector_root(rhs: &str) -> &str {
    let is_word = |c: char| c.is_alphanumeric() || matches!(c, '_' | '$');
    let (root, rest) = rhs.split_at(rhs.find(|c: char| !is_word(c)).unwrap_or(rhs.len()));
    let mut depth = 0i32;
    let chain = rest.chars().all(|c| {
        match c {
            '(' | '[' => depth += 1,
            ')' | ']' => depth -= 1,
            _ => {}
        }
        depth > 0 || is_word(c) || matches!(c, '.' | '?' | '!' | '(' | '[' | ')' | ']')
    });
    if root.is_empty() || !chain { rhs } else { root }
}

const KINDS: ComplexityKinds = ComplexityKinds {
    paths: &["switch_statement_case", "switch_expression_case"],
    flat: &[],
    nesting: &[
        "if_statement",
        "for_statement",
        "while_statement",
        "do_statement",
        "catch_clause",
        "conditional_expression",
    ],
    nest_only: &["switch_statement"],
    else_kinds: &["else"],
    else_bodies: &[],
    jumps: &[
        "break_statement",
        "continue_statement",
        "return_statement",
        "rethrow_statement",
    ],
    logical_nodes: &["logical_and_expression", "logical_or_expression"],
    binary_nodes: &[],
    logical_ops: &[],
};

const SYNTAX: SyntaxKinds = SyntaxKinds {
    scope_containers: &[
        "class_definition",
        "enum_declaration",
        "mixin_declaration",
        "extension_declaration",
    ],
    scoped_paths: &["scoped_identifier"],
    member_access: &[
        "unconditional_assignable_selector",
        "conditional_assignable_selector",
    ],
    body_blocks: &["function_body", "block", "class_body"],
    body_wrappers: &["function_expression", "parenthesized_expression"],
    local_bindings: &[
        "initialized_identifier",
        "initialized_variable_definition",
        "static_final_declaration",
    ],
    non_local_declarations: &["enum_declaration"],
    identifiers: &["identifier"],
    type_contexts: &["type_parameters", "type_parameter", "type_arguments"],
    callables: &["function_expression"],
    callable_wrappers: &["parenthesized_expression"],
    signature_holders: &["method_signature", "declaration"],
    optional_param_groups: &["optional_formal_parameters"],
    binding_patterns: &["object_pattern", "rest_pattern"],
    ..SyntaxKinds::NONE
};

/// Dart parses `a.b!.c()` as sibling selectors; a `!` is its own childless selector.
fn selector_receiver(selector: tree_sitter::Node<'_>) -> Option<tree_sitter::Node<'_>> {
    let mut receiver = selector.prev_named_sibling()?;
    while receiver.kind() == "selector" && receiver.named_child_count() == 0 {
        receiver = receiver.prev_named_sibling()?;
    }
    Some(receiver)
}

fn dart_call_qualifier(name_node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let selector = name_node.parent()?.parent()?;
    if selector.kind() != "selector" {
        return None;
    }
    let receiver = selector_receiver(selector)?;
    let ident = match receiver.kind() {
        "identifier" => receiver,
        "selector" => receiver.named_child(0)?.named_child(0)?,
        _ => return None,
    };
    let text = ident.utf8_text(source).ok()?;
    text.starts_with(char::is_uppercase)
        .then(|| text.to_string())
}

/// The class identifier of a named constructor or factory (`C` in `C.seeded`): an identifier
/// directly followed by the member identifier in a constructor signature.
fn is_named_constructor_class_name(name_node: Option<tree_sitter::Node<'_>>) -> bool {
    name_node.is_some_and(|name| {
        name.parent().is_some_and(is_constructor_signature)
            && name
                .next_named_sibling()
                .is_some_and(|next| next.kind() == "identifier")
    })
}

/// An unnamed constructor or factory: its only identifier is the class name.
fn is_unnamed_constructor(name_node: Option<tree_sitter::Node<'_>>) -> bool {
    name_node.is_some_and(|name| {
        name.parent().is_some_and(is_constructor_signature) && name.prev_named_sibling().is_none()
    })
}

fn is_constructor_signature(node: tree_sitter::Node<'_>) -> bool {
    matches!(
        node.kind(),
        "constructor_signature" | "factory_constructor_signature"
    )
}

impl LanguageSemantics for DartLanguage {
    fn call_qualifier(
        &self,
        name_node: tree_sitter::Node<'_>,
        source_bytes: &[u8],
    ) -> Option<String> {
        dart_call_qualifier(name_node, source_bytes)
    }

    fn qualifier_must_be_indexed(&self) -> bool {
        true
    }

    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &KINDS
    }

    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }

    fn member_receiver<'a>(&self, access: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
        match access.kind() {
            "unconditional_assignable_selector" | "conditional_assignable_selector" => {
                selector_receiver(access.parent()?)
            }
            _ => None,
        }
    }

    fn call_receiver_name<'a>(
        &self,
        receiver: tree_sitter::Node<'_>,
        source: &'a [u8],
    ) -> Option<&'a str> {
        let is_argument_selector = receiver.kind() == "selector"
            && receiver
                .named_child(0)
                .is_some_and(|part| part.kind() == "argument_part");
        let callee = receiver
            .prev_named_sibling()
            .filter(|callee| is_argument_selector && callee.kind() == "identifier")?;
        callee.utf8_text(source).ok()
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

    fn tag(&self) -> &'static str {
        "dart"
    }

    fn lint_groups(&self) -> &'static [crate::languages::LintGroup] {
        &[crate::languages::LintGroup::Naming]
    }
    super::impl_grammar_method!(ffi tree_sitter_dart, include_str!("tags.scm"));

    fn lint_extra_rules(&self) -> &'static [&'static str] {
        &[
            "do-while",
            "self-assign",
            "unreachable-code",
            "redundant-parens",
        ]
    }

    fn assigned_value_root<'a>(&self, rhs: &'a str) -> &'a str {
        dart_selector_root(rhs)
    }

    fn type_declaration_keywords(&self) -> &'static [&'static str] {
        &["typedef"]
    }

    fn display_color(&self) -> &'static str {
        "#00b4ab"
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Isolated(Lang::Dart)
    }

    fn visibility_mode(&self) -> VisibilityMode {
        VisibilityMode::ByNameConvention
    }

    fn resolve_def_body_and_end<'a>(
        &self,
        def_node: Option<tree_sitter::Node<'a>>,
        default_end: usize,
    ) -> (Option<tree_sitter::Node<'a>>, usize) {
        def_node
            .and_then(dart_following_body)
            .map_or((def_node, default_end), |b| (Some(b), b.end_byte()))
    }

    fn is_def_capture_allowed(&self, ctx: &DefCaptureGateContext<'_>) -> bool {
        let function_in_signature = ctx.kind == SymKind::Function
            && ctx
                .def_node
                .and_then(|dn| dn.parent())
                .is_some_and(|p| p.kind() == "method_signature");
        !(function_in_signature || is_named_constructor_class_name(ctx.name_node))
    }

    /// An unnamed constructor or factory (`C()`) shares its class's name. Only by-name calls to
    /// `C` reach it, and they bind to the class: the constructor is excluded from those candidates.
    fn is_contract_member(&self, ctx: &DefCaptureGateContext<'_>) -> bool {
        ctx.kind == SymKind::Method && is_unnamed_constructor(ctx.name_node)
    }

    fn is_symbol_name_public(&self, sym_name: &str) -> bool {
        !sym_name.starts_with('_') && !sym_name.starts_with('#')
    }

    fn should_ignore_call(&self, _name_node: Option<tree_sitter::Node<'_>>, name: &str) -> bool {
        matches!(name, "print" | "identical" | "jsonEncode" | "jsonDecode")
    }

    fn std_member_names(&self) -> &'static [&'static str] {
        DART_STD_MEMBERS
    }

    fn std_qualifier_names(&self) -> &'static [&'static str] {
        DART_STD_QUALIFIERS
    }

    fn resolve_import_target(
        &self,
        importer_path: &str,
        raw_target: &str,
        file_index: &HashMap<&str, u32>,
    ) -> Option<u32> {
        let clean = super::strip_import_quotes(raw_target);
        if clean.is_empty() || clean.starts_with("dart:") {
            return None;
        }
        if let Some(pkg_spec) = clean.strip_prefix("package:") {
            return resolve_dart_package_import(importer_path, pkg_spec, file_index);
        }
        super::resolve_relative_or_exact(importer_path, clean, file_index, &[".dart"])
    }
}
