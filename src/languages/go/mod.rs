use super::{ComplexityKinds, DialectGroup, LanguageSemantics, SyntaxKinds, VisibilityMode};
use crate::domain::{Lang, SymKind};

pub struct GoLanguage;
pub static GO_LANG: GoLanguage = GoLanguage;

const GO_STD_MEMBERS: &[&str] = &[
    "Error",
    "String",
    "Read",
    "Write",
    "Close",
    "Flush",
    "Seek",
    "Lock",
    "Unlock",
    "RLock",
    "RUnlock",
    "Add",
    "Done",
    "Wait",
    "Load",
    "Store",
    "Swap",
    "CompareAndSwap",
    "Context",
    "Deadline",
    "Err",
    "Value",
    "Len",
    "Less",
    "MarshalJSON",
    "UnmarshalJSON",
    "Scan",
];

const GO_STD_QUALIFIERS: &[&str] = &[
    "fmt", "os", "io", "strings", "bytes", "strconv", "time", "context", "sync", "atomic",
    "errors", "filepath", "path", "http", "url", "json", "xml", "sort", "slices", "maps", "math",
    "rand", "reflect", "regexp", "bufio", "log", "slog", "testing", "exec", "syscall", "runtime",
    "binary", "hex", "base64", "sha256", "tls", "net", "sql",
];

const KINDS: ComplexityKinds = ComplexityKinds {
    paths: &[
        "expression_case",
        "type_case",
        "communication_case",
        "default_case",
    ],
    flat: &[],
    nesting: &["if_statement", "for_statement"],
    nest_only: &[
        "expression_switch_statement",
        "type_switch_statement",
        "select_statement",
    ],
    else_kinds: &["else"],
    else_bodies: &[],
    jumps: &[
        "break_statement",
        "continue_statement",
        "goto_statement",
        "return_statement",
    ],
    logical_nodes: &[],
    binary_nodes: &["binary_expression"],
    logical_ops: &["&&", "||"],
};

const SYNTAX: SyntaxKinds = SyntaxKinds {
    member_access: &["selector_expression"],
    body_blocks: &["block", "field_declaration_list"],
    body_wrappers: &[
        "parenthesized_expression",
        "assignment_statement",
        "expression_list",
    ],
    function_defs: &["function_declaration"],
    local_bindings: &["range_clause"],
    non_local_declarations: &["function_declaration"],
    identifiers: &["identifier"],
    type_contexts: &["type_arguments", "generic_type"],
    callable_wrappers: &["parenthesized_expression"],
    ..SyntaxKinds::NONE
};

/// Parameters a Go parameter list declares: each `a, b int` declaration counts its names, an
/// unnamed `int` counts once. The receiver of a method is a separate list and is not counted.
fn go_param_count(list: tree_sitter::Node<'_>) -> u16 {
    let mut cursor = list.walk();
    list.named_children(&mut cursor)
        .map(|decl| {
            let names = match decl.kind() {
                "parameter_declaration" => {
                    let mut name_cursor = decl.walk();
                    decl.children_by_field_name("name", &mut name_cursor)
                        .count()
                        .max(1)
                }
                "variadic_parameter_declaration" => 1,
                _ => 0,
            };
            u16::try_from(names).unwrap_or(u16::MAX)
        })
        .sum()
}

impl LanguageSemantics for GoLanguage {
    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &KINDS
    }

    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }

    fn tag(&self) -> &'static str {
        "go"
    }

    fn lint_groups(&self) -> &'static [crate::languages::LintGroup] {
        use crate::languages::LintGroup as G;
        &[G::Goto, G::Insecure, G::Parens, G::Unreachable, G::Naming]
    }

    fn type_declaration_keywords(&self) -> &'static [&'static str] {
        &["type"]
    }

    fn is_generated_file_name(&self, file_name: &str) -> bool {
        file_name.ends_with(".pb.go")
    }
    super::impl_grammar_method!(crate tree_sitter_go::LANGUAGE, "tags.scm");

    fn display_color(&self) -> &'static str {
        "#00add8"
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Isolated(Lang::Go)
    }

    fn visibility_mode(&self) -> VisibilityMode {
        VisibilityMode::ByNameConvention
    }

    fn is_symbol_name_public(&self, sym_name: &str) -> bool {
        sym_name.chars().next().is_some_and(|c| c.is_uppercase())
    }

    fn check_node_visibility(&self, def_node: tree_sitter::Node<'_>, source_bytes: &[u8]) -> bool {
        let Some(name_node) = def_node.child_by_field_name("name") else {
            return false;
        };
        let Ok(text) = name_node.utf8_text(source_bytes) else {
            return false;
        };
        text.chars().next().is_some_and(|c| c.is_uppercase())
    }

    fn extract_custom_scope(
        &self,
        parent: tree_sitter::Node<'_>,
        _name_node: tree_sitter::Node<'_>,
        source_bytes: &[u8],
    ) -> Option<String> {
        if parent.kind() != "method_declaration" {
            return None;
        }
        let recv_list = parent.child_by_field_name("receiver")?;
        let param = recv_list.named_child(0)?;
        let type_node = param.child_by_field_name("type")?;
        let text = type_node.utf8_text(source_bytes).ok()?;
        let base = text
            .trim_start_matches('*')
            .split('[')
            .next()
            .unwrap_or("")
            .trim();
        (!base.is_empty()).then(|| base.to_string())
    }

    fn resolve_def_params_and_name(
        &self,
        def_node: Option<tree_sitter::Node<'_>>,
        _source_bytes: &[u8],
        _kind: SymKind,
        orig_name: &str,
    ) -> (crate::gate::contract::ParamArity, String) {
        let arity = def_node.map_or(crate::gate::contract::ParamArity::default(), |n| {
            match n.child_by_field_name("parameters") {
                Some(list) => crate::gate::contract::ParamArity::fixed(go_param_count(list)),
                None => crate::gate::contract::extract_param_arity(n, self.syntax_kinds()),
            }
        });
        (arity, orig_name.to_string())
    }

    fn std_member_names(&self) -> &'static [&'static str] {
        GO_STD_MEMBERS
    }

    fn std_qualifier_names(&self) -> &'static [&'static str] {
        GO_STD_QUALIFIERS
    }
}
