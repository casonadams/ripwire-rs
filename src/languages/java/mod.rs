use super::{
    ComplexityKinds, DefCaptureGateContext, DialectGroup, LanguageSemantics, SyntaxKinds,
    VisibilityMode,
};
use crate::languages::is_plain_type_name;
use std::collections::HashMap;

pub struct JavaLanguage;
pub static JAVA_LANG: JavaLanguage = JavaLanguage;

pub(super) const JVM_STD_MEMBERS: &[&str] = &[
    "get",
    "set",
    "put",
    "remove",
    "add",
    "addAll",
    "clear",
    "contains",
    "containsKey",
    "size",
    "isEmpty",
    "iterator",
    "stream",
    "map",
    "filter",
    "flatMap",
    "forEach",
    "collect",
    "toList",
    "toArray",
    "orElse",
    "orElseGet",
    "orElseThrow",
    "isPresent",
    "equals",
    "hashCode",
    "toString",
    "compareTo",
    "length",
    "charAt",
    "substring",
    "split",
    "trim",
    "replace",
    "startsWith",
    "endsWith",
    "toLowerCase",
    "toUpperCase",
    "format",
    "valueOf",
    "parseInt",
    "parseLong",
    "append",
];

pub(super) const JVM_STD_QUALIFIERS: &[&str] = &[
    "System",
    "Math",
    "String",
    "Integer",
    "Long",
    "Double",
    "Boolean",
    "Objects",
    "Collections",
    "Arrays",
    "Optional",
    "Stream",
    "Collectors",
    "List",
    "Map",
    "Set",
    "Files",
    "Paths",
    "Path",
    "UUID",
    "Instant",
    "Duration",
    "LocalDate",
    "LocalDateTime",
    "CompletableFuture",
    "Thread",
    "Executors",
    "LoggerFactory",
];

const KINDS: ComplexityKinds = ComplexityKinds {
    paths: &["switch_block_statement_group", "switch_rule"],
    flat: &[],
    nesting: &[
        "if_statement",
        "for_statement",
        "enhanced_for_statement",
        "while_statement",
        "do_statement",
        "catch_clause",
        "ternary_expression",
    ],
    nest_only: &["switch_expression"],
    else_kinds: &["else"],
    else_bodies: &[],
    jumps: &[
        "break_statement",
        "continue_statement",
        "return_statement",
        "throw_statement",
        "yield_statement",
    ],
    logical_nodes: &[],
    binary_nodes: &["binary_expression"],
    logical_ops: &["&&", "||"],
};

const SYNTAX: SyntaxKinds = SyntaxKinds {
    scope_containers: &[
        "class_declaration",
        "record_declaration",
        "enum_declaration",
        "interface_declaration",
    ],
    scoped_paths: &["scoped_identifier"],
    member_access: &["method_invocation"],
    body_blocks: &["block", "class_body"],
    body_wrappers: &["variable_declarator", "parenthesized_expression"],
    non_local_declarations: &[
        "class_declaration",
        "interface_declaration",
        "enum_declaration",
    ],
    identifiers: &["identifier"],
    type_contexts: &[
        "type_parameters",
        "type_parameter",
        "type_arguments",
        "generic_type",
    ],
    callable_wrappers: &["variable_declarator", "parenthesized_expression"],
    ..SyntaxKinds::NONE
};

const TYPED: super::typed::TypedSyntax = super::typed::TypedSyntax {
    classes: &[
        "class_declaration",
        "interface_declaration",
        "enum_declaration",
        "record_declaration",
    ],
    supertype_clauses: &["superclass", "super_interfaces", "extends_interfaces"],
    params: &["formal_parameter", "spread_parameter", "enhanced_for_statement"],
    functions: &["method_declaration", "constructor_declaration", "lambda_expression"],
    locals: &["local_variable_declaration"],
    fields: &["field_declaration"],
    declarators: &["variable_declarator"],
    blocks: &["block", "constructor_body"],
    constructions: &["object_creation_expression"],
    inferred: &["var"],
    assignments: &[],
    calls: &[("method_invocation", "name")],
    return_type_field: "type",
    superclass: Some(("superclass", "super")),
    declaration_name: None,
    class_params: &[],
    transparent_wrappers: &[],
    scoped_nesting: true,
};

impl LanguageSemantics for JavaLanguage {
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
    fn import_names_full_path(&self) -> bool {
        true
    }
    fn import_module_bindings(&self, raw_target: &str) -> Vec<(String, String)> {
        raw_target.rsplit_once('.').map_or_else(Vec::new, |(_, bound)| {
            vec![(raw_target.to_string(), bound.to_string())]
        })
    }
    fn resolve_import_target(
        &self,
        importer_path: &str,
        raw_target: &str,
        file_index: &HashMap<&str, u32>,
    ) -> Option<u32> {
        java_class_file(importer_path, raw_target, file_index).or_else(|| {
            // `import static a.b.C.m;` names a member of the class `a.b.C`.
            let (class, _) = raw_target.rsplit_once('.')?;
            java_class_file(importer_path, class, file_index)
        })
    }
    fn receiver_chain(&self, receiver: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
        java_receiver_chain(receiver, source)
    }
    fn tag(&self) -> &'static str {
        "java"
    }

    fn lint_groups(&self) -> &'static [crate::languages::LintGroup] {
        use crate::languages::LintGroup as G;
        &[G::Cast, G::DoWhile, G::Parens, G::SelfAssign, G::Unreachable, G::Naming]
    }
    super::impl_grammar_method!(crate tree_sitter_java::LANGUAGE, "tags.scm");

    fn lint_return_type<'a>(&self, sig: &'a str, name: &str) -> Option<&'a str> {
        super::lint_hooks::prefix_return_type(sig, name)
    }

    fn type_declaration_keywords(&self) -> &'static [&'static str] {
        &["enum"]
    }

    fn display_color(&self) -> &'static str {
        "#b07219"
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Jvm
    }

    fn visibility_mode(&self) -> VisibilityMode {
        VisibilityMode::ExplicitModifier
    }

    fn is_entrypoint_symbol(
        &self,
        sym_name: &str,
        _scope: Option<&str>,
        kind: crate::domain::SymKind,
        rel_path: &str,
    ) -> bool {
        kind.is_callable() && !super::is_non_entrypoint_path(rel_path) && sym_name == "main"
    }

    fn is_def_capture_allowed(&self, ctx: &DefCaptureGateContext<'_>) -> bool {
        ctx.sub != Some("constant") || super::is_screaming_snake(ctx.name)
    }

    fn std_member_names(&self) -> &'static [&'static str] {
        JVM_STD_MEMBERS
    }

    fn std_qualifier_names(&self) -> &'static [&'static str] {
        JVM_STD_QUALIFIERS
    }
}

/// Source roots a Java package path may sit under, relative to an ancestor of the importer.
const JAVA_SOURCE_ROOTS: &[&str] = &["", "src/main/java/", "src/test/java/", "src/", "java/"];

/// The file declaring the class `dotted` (`a.b.C` is `a/b/C.java`), probed under the importer's directory
/// and each ancestor, nearest first, and under each conventional source root.
fn java_class_file(
    importer_path: &str,
    dotted: &str,
    file_index: &HashMap<&str, u32>,
) -> Option<u32> {
    if dotted.is_empty() || dotted.contains('*') {
        return None;
    }
    let rel = format!("{}.java", dotted.replace('.', "/"));
    let mut dir = importer_path.rsplit_once('/').map_or("", |(d, _)| d);
    loop {
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        if let Some(&id) = JAVA_SOURCE_ROOTS
            .iter()
            .find_map(|root| file_index.get(format!("{prefix}{root}{rel}").as_str()))
        {
            return Some(id);
        }
        if dir.is_empty() {
            return None;
        }
        dir = dir.rsplit_once('/').map_or("", |(d, _)| d);
    }
}

/// A receiver's text as `receiver_type_at` reads it: `a.b()` for a call on a dotted path, `(T)` for a
/// cast to a plain type, and the chain of both (`a.b().c()` names `a.b()`).
fn java_receiver_chain(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    match node.kind() {
        "parenthesized_expression" => java_receiver_chain(node.named_child(0)?, source),
        "cast_expression" => {
            let ty = node.child_by_field_name("type")?.utf8_text(source).ok()?;
            is_plain_type_name(ty).then(|| format!("({ty})"))
        }
        "method_invocation" => {
            let name = node.child_by_field_name("name")?.utf8_text(source).ok()?;
            let Some(object) = node.child_by_field_name("object") else {
                return Some(format!("{name}()"));
            };
            let base = java_receiver_chain(object, source)?;
            Some(format!("{base}.{name}()"))
        }
        _ => dotted_path(node, source),
    }
}

/// A dotted name of identifiers (`doc`, `this.tag`), as written.
fn dotted_path(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    let text = node.utf8_text(source).ok()?;
    text.split('.')
        .all(|segment| {
            !segment.is_empty()
                && segment
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_')
        })
        .then(|| text.to_string())
}
