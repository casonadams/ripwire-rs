use super::elixir::{BEAM_STD_MEMBERS, BEAM_STD_QUALIFIERS};
use super::reads::nodes;
use super::{ComplexityKinds, DialectGroup, LanguageSemantics, SyntaxKinds};

pub struct ErlangLanguage;
pub static ERLANG_LANG: ErlangLanguage = ErlangLanguage;

/// Auto-imported BIFs of the `erlang` module. A bare call of one names `erlang:Name`, never a
/// function of another module (a local function of the same name needs `no_auto_import`).
const ERLANG_AUTO_IMPORTED_BIFS: &[&str] = &[
    "abs",
    "apply",
    "atom_to_binary",
    "atom_to_list",
    "binary_part",
    "binary_to_atom",
    "binary_to_existing_atom",
    "binary_to_float",
    "binary_to_integer",
    "binary_to_list",
    "bit_size",
    "byte_size",
    "ceil",
    "element",
    "erase",
    "error",
    "exit",
    "float",
    "float_to_binary",
    "float_to_list",
    "floor",
    "get",
    "group_leader",
    "halt",
    "hd",
    "integer_to_binary",
    "integer_to_list",
    "is_atom",
    "is_binary",
    "is_bitstring",
    "is_boolean",
    "is_float",
    "is_function",
    "is_integer",
    "is_list",
    "is_map",
    "is_number",
    "is_pid",
    "is_port",
    "is_process_alive",
    "is_record",
    "is_reference",
    "is_tuple",
    "length",
    "link",
    "list_to_atom",
    "list_to_binary",
    "list_to_bitstring",
    "list_to_existing_atom",
    "list_to_float",
    "list_to_integer",
    "list_to_pid",
    "list_to_tuple",
    "make_ref",
    "map_get",
    "map_size",
    "max",
    "min",
    "monitor",
    "node",
    "nodes",
    "now",
    "open_port",
    "put",
    "register",
    "registered",
    "round",
    "self",
    "setelement",
    "size",
    "spawn",
    "spawn_link",
    "spawn_monitor",
    "spawn_opt",
    "split_binary",
    "term_to_binary",
    "throw",
    "tl",
    "trunc",
    "tuple_size",
    "tuple_to_list",
    "unlink",
    "unregister",
    "whereis",
];

const KINDS: ComplexityKinds = ComplexityKinds {
    paths: &["cr_clause", "if_clause"],
    flat: &["catch_clause"],
    nesting: &["try_expr", "catch_expr"],
    nest_only: &["case_expr", "if_expr", "receive_expr"],
    else_kinds: &[],
    else_bodies: &[],
    jumps: &[],
    logical_nodes: &[],
    binary_nodes: &["binary_op_expr"],
    logical_ops: &["andalso", "orelse"],
};

const SYNTAX: SyntaxKinds = SyntaxKinds {
    scope_containers: &["module"],
    member_access: &["remote"],
    body_blocks: &["clause_body"],
    ..SyntaxKinds::NONE
};

/// True when `name_node` is the callee of a bare call of an auto-imported BIF that the module does
/// not define itself.
fn is_bif_call(name_node: tree_sitter::Node<'_>, source_bytes: &[u8]) -> bool {
    let Ok(name) = name_node.utf8_text(source_bytes) else {
        return false;
    };
    let bare_call = name_node
        .parent()
        .is_some_and(|p| p.kind() == "call" && p.child_by_field_name("expr") == Some(name_node));
    bare_call
        && ERLANG_AUTO_IMPORTED_BIFS.contains(&name)
        && !module_defines(name_node, name, source_bytes)
}

fn module_defines(node: tree_sitter::Node<'_>, name: &str, source_bytes: &[u8]) -> bool {
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    nodes(root).any(|n| {
        n.kind() == "function_clause"
            && n.child_by_field_name("name")
                .is_some_and(|f| f.utf8_text(source_bytes) == Ok(name))
    })
}

impl LanguageSemantics for ErlangLanguage {
    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &KINDS
    }

    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }

    fn member_receiver<'a>(&self, access: tree_sitter::Node<'a>) -> Option<tree_sitter::Node<'a>> {
        match access.kind() {
            "remote" => access
                .child_by_field_name("module")
                .and_then(|m| m.child_by_field_name("module")),
            _ => None,
        }
    }

    /// A bare BIF call (`length(X)`) is `erlang:length/1`, not a same-named function of another module.
    fn call_qualifier(
        &self,
        name_node: tree_sitter::Node<'_>,
        source_bytes: &[u8],
    ) -> Option<String> {
        is_bif_call(name_node, source_bytes).then(|| "erlang".to_string())
    }

    fn tag(&self) -> &'static str {
        "erl"
    }

    super::impl_grammar_method!(crate tree_sitter_erlang::LANGUAGE, "tags.scm");

    fn extensions(&self) -> &'static [&'static str] {
        &["erl", "hrl"]
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Beam
    }

    fn std_member_names(&self) -> &'static [&'static str] {
        BEAM_STD_MEMBERS
    }

    fn std_qualifier_names(&self) -> &'static [&'static str] {
        BEAM_STD_QUALIFIERS
    }
}
