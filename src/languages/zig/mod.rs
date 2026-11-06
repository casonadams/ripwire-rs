use super::{
    ComplexityKinds, DialectGroup, LanguageSemantics, SyntaxKinds, resolve_relative_or_exact,
    strip_import_quotes,
};
use crate::domain::Lang;
use std::collections::HashMap;

pub struct ZigLanguage;
pub static ZIG_LANG: ZigLanguage = ZigLanguage;

const ZIG_STD_MEMBERS: &[&str] = &[
    "init",
    "deinit",
    "alloc",
    "free",
    "create",
    "destroy",
    "dupe",
    "append",
    "appendSlice",
    "pop",
    "clearRetainingCapacity",
    "clearAndFree",
    "toOwnedSlice",
    "get",
    "put",
    "remove",
    "contains",
    "iterator",
    "next",
    "format",
    "print",
    "read",
    "write",
    "close",
];

const ZIG_STD_QUALIFIERS: &[&str] = &[
    "std",
    "mem",
    "fs",
    "io",
    "os",
    "posix",
    "fmt",
    "math",
    "debug",
    "testing",
    "heap",
    "time",
    "Thread",
    "ArrayList",
    "StringHashMap",
    "AutoHashMap",
];

const KINDS: ComplexityKinds = ComplexityKinds {
    paths: &["switch_case"],
    flat: &[],
    nesting: &[
        "if_expression",
        "if_statement",
        "for_expression",
        "for_statement",
        "while_expression",
        "while_statement",
        "catch_expression",
    ],
    nest_only: &["switch_expression"],
    else_kinds: &["else_clause"],
    else_bodies: &[],
    jumps: &[
        "break_expression",
        "continue_expression",
        "return_expression",
    ],
    logical_nodes: &[],
    binary_nodes: &["binary_expression"],
    logical_ops: &["and", "or", "orelse"],
};

const SYNTAX: SyntaxKinds = SyntaxKinds {
    scope_containers: &["struct_declaration", "enum_declaration"],
    member_access: &["field_expression"],
    field_access: &["field_expression"],
    body_blocks: &["block"],
    body_wrappers: &["variable_declaration", "parenthesized_expression"],
    function_defs: &["function_declaration"],
    non_local_declarations: &["function_declaration", "enum_declaration"],
    identifiers: &["identifier"],
    callable_wrappers: &["parenthesized_expression"],
    ..SyntaxKinds::NONE
};


/// The module path and the name it binds in `const util = @import("util.zig");` (a bare `@import("x")` binds
/// nothing). The text is the import node's source, or a plain path. None unless the path names a `.zig` file.
fn zig_import(raw: &str) -> Option<(&str, Option<&str>)> {
    let Some((decl, call)) = raw.split_once("@import(") else {
        let path = strip_import_quotes(raw);
        return path.ends_with(".zig").then_some((path, None));
    };
    let path = call.split('"').nth(1)?;
    let bound = decl
        .split_once('=')
        .and_then(|(lhs, _)| lhs.split_whitespace().last())
        .filter(|name| name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_'));
    path.ends_with(".zig").then_some((path, bound))
}

impl LanguageSemantics for ZigLanguage {
    /// A struct, union or enum declared as `const Name = struct { ... };` scopes its members to `Name`.
    fn extract_custom_scope(
        &self,
        parent: tree_sitter::Node<'_>,
        _name_node: tree_sitter::Node<'_>,
        source_bytes: &[u8],
    ) -> Option<String> {
        if !matches!(
            parent.kind(),
            "struct_declaration" | "union_declaration" | "enum_declaration"
        ) {
            return None;
        }
        let decl = parent
            .parent()
            .filter(|p| p.kind() == "variable_declaration")?;
        let name = (0..decl.named_child_count())
            .filter_map(|i| decl.named_child(i))
            .find(|c| c.kind() == "identifier")?;
        name.utf8_text(source_bytes).ok().map(str::to_string)
    }

    fn import_binding<'a>(&self, raw_target: &'a str) -> Option<(&'a str, &'a str)> {
        zig_import(raw_target).and_then(|(path, bound)| Some((path, bound?)))
    }

    fn module_member_qualifiers(&self) -> bool {
        true
    }

    fn import_module_bindings(&self, raw_target: &str) -> Vec<(String, String)> {
        match zig_import(raw_target) {
            Some((path, Some(bound))) => vec![(path.to_string(), bound.to_string())],
            _ => Vec::new(),
        }
    }

    fn resolve_import_target(
        &self,
        importer_path: &str,
        raw_target: &str,
        file_index: &HashMap<&str, u32>,
    ) -> Option<u32> {
        let (path, _) = zig_import(raw_target)?;
        resolve_relative_or_exact(importer_path, path, file_index, &[])
    }

    /// `util.log(v)` names its module by the receiver identifier, which binds an import (`const util = @import`).
    /// A receiver that binds no module falls back to name resolution in the resolver.
    fn call_qualifier(&self, name_node: tree_sitter::Node<'_>, source_bytes: &[u8]) -> Option<String> {
        let access = name_node
            .parent()
            .filter(|p| p.kind() == "field_expression")?;
        let object = access.child_by_field_name("object")?;
        let object = match object.kind() {
            "expression" => object.named_child(0)?,
            _ => object,
        };
        (object.kind() == "identifier")
            .then(|| object.utf8_text(source_bytes).ok().map(str::to_string))
            .flatten()
    }

    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &KINDS
    }

    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }

    fn tag(&self) -> &'static str {
        "zig"
    }
    super::impl_grammar_method!(crate tree_sitter_zig::LANGUAGE, "tags.scm");

    fn extensions(&self) -> &'static [&'static str] {
        &["zig", "zon"]
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Isolated(Lang::Zig)
    }

    fn std_member_names(&self) -> &'static [&'static str] {
        ZIG_STD_MEMBERS
    }

    fn std_qualifier_names(&self) -> &'static [&'static str] {
        ZIG_STD_QUALIFIERS
    }
}
