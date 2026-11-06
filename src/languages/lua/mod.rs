use super::{ComplexityKinds, DialectGroup, LanguageSemantics, SyntaxKinds, VisibilityMode};
use crate::domain::Lang;
use std::collections::HashMap;

pub struct LuaLanguage;
pub static LUA_LANG: LuaLanguage = LuaLanguage;

const LUA_STD_MEMBERS: &[&str] = &[
    "insert", "remove", "concat", "sort", "unpack", "pack", "sub", "find", "match", "gmatch",
    "gsub", "rep", "reverse", "lower", "upper", "len", "byte", "char", "format", "floor", "ceil",
    "max", "min", "abs", "sqrt", "open", "close", "read", "write", "lines", "flush",
];

const LUA_STD_QUALIFIERS: &[&str] = &[
    "table",
    "string",
    "math",
    "io",
    "os",
    "coroutine",
    "package",
    "debug",
    "utf8",
    "bit",
    "vim",
];

/// The module a `local x = require "mod"` (or `require("mod")`) declaration loads, and the name it binds.
/// None for anything else: `require "m" . field` binds a field of the module, not the module.
fn lua_require(raw: &str) -> Option<(&str, &str)> {
    let (lhs, rhs) = raw.trim().strip_prefix("local")?.split_once('=')?;
    let bound = lhs.trim();
    if bound.is_empty() || !bound.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_') {
        return None;
    }
    let call = rhs.trim().strip_prefix("require")?.trim_start();
    let (args, parenthesized) = match call.strip_prefix('(') {
        Some(inner) => (inner.trim_start(), true),
        None => (call, false),
    };
    let quote = args.chars().next().filter(|c| *c == '"' || *c == '\'')?;
    let body = &args[1..];
    let end = body.find(quote)?;
    let module = &body[..end];
    let tail = body[end + 1..].trim();
    let closed = if parenthesized { tail == ")" } else { tail.is_empty() };
    let valid = !module.is_empty()
        && module
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-'));
    (closed && valid).then_some((module, bound))
}

/// The `.lua` file a dotted module names: `pl.url` is `pl/url.lua` or `pl/url/init.lua` under any root.
/// Unique matches only; two candidates decline rather than pick one.
fn lua_module_file(module: &str, file_index: &HashMap<&str, u32>) -> Option<u32> {
    let path = module.replace('.', "/");
    let suffixes = [format!("{path}.lua"), format!("{path}/init.lua")];
    let mut found = file_index
        .iter()
        .filter(|(file, _)| {
            suffixes
                .iter()
                .any(|sfx| *file == sfx || file.ends_with(&format!("/{sfx}")))
        })
        .map(|(_, id)| *id);
    let first = found.next()?;
    found.next().is_none().then_some(first)
}

const KINDS: ComplexityKinds = ComplexityKinds {
    paths: &[],
    flat: &["elseif_statement"],
    nesting: &[
        "if_statement",
        "for_statement",
        "while_statement",
        "repeat_statement",
    ],
    nest_only: &[],
    else_kinds: &["else_statement"],
    else_bodies: &[],
    jumps: &["break_statement", "goto_statement", "return_statement"],
    logical_nodes: &[],
    binary_nodes: &["binary_expression"],
    logical_ops: &["and", "or"],
};

const SYNTAX: SyntaxKinds = SyntaxKinds {
    member_access: &["dot_index_expression", "method_index_expression"],
    body_blocks: &["block"],
    body_wrappers: &[
        "variable_declaration",
        "function_definition",
        "parenthesized_expression",
        "assignment_statement",
        "expression_list",
        "table_constructor",
        "field",
    ],
    function_defs: &["function_declaration", "function_definition"],
    non_local_declarations: &["function_declaration"],
    identifiers: &["identifier"],
    callable_wrappers: &["parenthesized_expression"],
    ..SyntaxKinds::NONE
};

impl LanguageSemantics for LuaLanguage {
    fn complexity_kinds(&self) -> &'static ComplexityKinds {
        &KINDS
    }

    fn syntax_kinds(&self) -> &'static SyntaxKinds {
        &SYNTAX
    }

    fn tag(&self) -> &'static str {
        "lua"
    }

    fn lint_groups(&self) -> &'static [crate::languages::LintGroup] {
        &[crate::languages::LintGroup::Naming]
    }
    super::impl_grammar_method!(crate tree_sitter_lua::LANGUAGE, "tags.scm");

    fn display_color(&self) -> &'static str {
        "#000080"
    }

    fn dialect_group(&self) -> DialectGroup {
        DialectGroup::Isolated(Lang::Lua)
    }

    fn visibility_mode(&self) -> VisibilityMode {
        VisibilityMode::ByNameConvention
    }

    fn is_symbol_name_public(&self, sym_name: &str) -> bool {
        !sym_name.starts_with('_') && !sym_name.starts_with('#')
    }

    fn extract_custom_scope(
        &self,
        parent: tree_sitter::Node<'_>,
        name_node: tree_sitter::Node<'_>,
        source_bytes: &[u8],
    ) -> Option<String> {
        if !matches!(
            parent.kind(),
            "dot_index_expression" | "method_index_expression"
        ) {
            return None;
        }
        let gp_kind = parent.parent()?.kind();
        if !matches!(gp_kind, "function_declaration" | "variable_list") {
            return None;
        }
        crate::ingest::distinct_scope_tail(
            parent.child_by_field_name("table")?,
            name_node,
            source_bytes,
            "",
        )
    }

    fn import_binding<'a>(&self, raw_target: &'a str) -> Option<(&'a str, &'a str)> {
        lua_require(raw_target)
    }

    fn import_module_bindings(&self, raw_target: &str) -> Vec<(String, String)> {
        lua_require(raw_target)
            .map(|(module, bound)| vec![(module.to_string(), bound.to_string())])
            .unwrap_or_default()
    }

    fn resolve_import_target(
        &self,
        _importer_path: &str,
        raw_target: &str,
        file_index: &HashMap<&str, u32>,
    ) -> Option<u32> {
        let module = lua_require(raw_target).map_or(raw_target, |(module, _)| module);
        lua_module_file(module, file_index)
    }

    fn std_member_names(&self) -> &'static [&'static str] {
        LUA_STD_MEMBERS
    }

    fn std_qualifier_names(&self) -> &'static [&'static str] {
        LUA_STD_QUALIFIERS
    }
}
