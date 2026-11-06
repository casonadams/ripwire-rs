use super::{ComplexityKinds, DialectGroup, Implementation, LanguageSemantics, SyntaxKinds};
use crate::domain::Lang;
use crate::languages::common::typed::implementation;
use crate::languages::common::reads::nodes;

pub struct SolidityLanguage;
pub static SOLIDITY_LANG: SolidityLanguage = SolidityLanguage;

/// Global builtins and members of the language's standard types. A bare `require(..)` or `keccak256(..)` names the
/// builtin, never a same-named function declared in the tree.
const SOLIDITY_STD_MEMBERS: &[&str] = &[
    "require",
    "assert",
    "revert",
    "keccak256",
    "sha256",
    "ripemd160",
    "ecrecover",
    "addmod",
    "mulmod",
    "selfdestruct",
    "blockhash",
    "gasleft",
    "push",
    "pop",
    "transfer",
    "send",
    "call",
    "delegatecall",
    "staticcall",
    "encode",
    "encodePacked",
    "encodeWithSelector",
    "encodeWithSignature",
    "encodeCall",
    "decode",
    "concat",
];

const SOLIDITY_STD_QUALIFIERS: &[&str] =
    &["abi", "msg", "block", "tx", "address", "string", "bytes"];

const KINDS: ComplexityKinds = ComplexityKinds {
    paths: &[],
    flat: &[],
    nesting: &[
        "if_statement",
        "for_statement",
        "while_statement",
        "do_while_statement",
        "catch_clause",
        "ternary_expression",
        "yul_if_statement",
        "yul_for_statement",
        "yul_switch_statement",
    ],
    nest_only: &[],
    else_kinds: &["else"],
    else_bodies: &[],
    jumps: &[
        "break_statement",
        "continue_statement",
        "return_statement",
        "revert_statement",
        "yul_break",
        "yul_continue",
        "yul_leave",
    ],
    logical_nodes: &[],
    binary_nodes: &["binary_expression"],
    logical_ops: &["&&", "||"],
};

const SYNTAX: SyntaxKinds = SyntaxKinds {
    scope_containers: &[
        "struct_declaration",
        "enum_declaration",
        "interface_declaration",
        "contract_declaration",
        "library_declaration",
    ],
    member_access: &["member_expression"],
    field_access: &["member_expression"],
    body_blocks: &["function_body"],
    body_wrappers: &["variable_declaration", "parenthesized_expression"],
    function_defs: &["function_definition"],
    non_local_declarations: &["interface_declaration", "enum_declaration"],
    identifiers: &["identifier"],
    callable_wrappers: &["parenthesized_expression"],
    ..SyntaxKinds::NONE
};

/// The contracts, interfaces and libraries a source declares, each with its own span and the parents it names
/// with `is`. A declaration is recorded as its own subject, so its members resolve to it, and each parent as a
/// supertype, so an inherited member resolves through the chain.
fn contract_implementations(root: tree_sitter::Node<'_>, source: &[u8]) -> Vec<Implementation> {
    nodes(root)
        .filter(|node| {
            matches!(
                node.kind(),
                "contract_declaration" | "interface_declaration" | "library_declaration"
            )
        })
        .filter_map(|decl| {
            let subject = decl.child_by_field_name("name")?.utf8_text(source).ok()?;
            let span = (decl.start_byte(), decl.end_byte());
            let mut cursor = decl.walk();
            let parents: Vec<Implementation> = decl
                .children(&mut cursor)
                .filter(|child| child.kind() == "inheritance_specifier")
                .filter_map(|spec| {
                    let ancestor = spec.child_by_field_name("ancestor")?;
                    let text = ancestor.utf8_text(source).ok()?;
                    let leaf = text.rsplit('.').next()?.trim();
                    (!leaf.is_empty()).then(|| implementation(subject, Some(leaf), span))
                })
                .collect();
            Some(std::iter::once(implementation(subject, None, span)).chain(parents))
        })
        .flatten()
        .collect()
}

impl LanguageSemantics for SolidityLanguage {
    fn implementations(
        &self,
        root: tree_sitter::Node<'_>,
        source: &[u8],
    ) -> Vec<Implementation> {
        contract_implementations(root, source)
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

    fn overloads_by_arity(&self) -> bool {
        true
    }

    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &KINDS
    }

    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }

    fn tag(&self) -> &'static str {
        "sol"
    }
    super::impl_grammar_method!(crate tree_sitter_solidity::LANGUAGE, "tags.scm");

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Isolated(Lang::Solidity)
    }

    fn std_member_names(&self) -> &'static [&'static str] {
        SOLIDITY_STD_MEMBERS
    }

    fn std_qualifier_names(&self) -> &'static [&'static str] {
        SOLIDITY_STD_QUALIFIERS
    }
}
