use crate::domain::Binding;
use super::common::util::universal_test_partner_subject;
use super::{
    ComplexityKinds, DefCaptureGateContext, DialectGroup, LanguageSemantics, SyntaxKinds,
    probe_with_suffixes, resolve_relative_or_exact, strip_import_quotes,
};
use std::collections::HashMap;

pub struct JavaScriptLanguage;
pub static JAVASCRIPT_LANG: JavaScriptLanguage = JavaScriptLanguage;

const WEB_SUFFIXES: &[&str] = &[
    ".ts",
    ".tsx",
    "/index.ts",
    "/index.tsx",
    ".js",
    ".jsx",
    "/index.js",
    ".d.ts",
];

pub(super) const JS_STD_MEMBERS: &[&str] = &[
    "push",
    "pop",
    "shift",
    "unshift",
    "slice",
    "splice",
    "map",
    "filter",
    "reduce",
    "forEach",
    "find",
    "findIndex",
    "includes",
    "indexOf",
    "join",
    "split",
    "trim",
    "replace",
    "replaceAll",
    "startsWith",
    "endsWith",
    "toLowerCase",
    "toUpperCase",
    "toString",
    "valueOf",
    "keys",
    "values",
    "entries",
    "then",
    "catch",
    "finally",
    "test",
    "match",
    "has",
    "get",
    "set",
    "delete",
    "add",
    "clear",
    "some",
    "every",
    "flat",
    "flatMap",
    "sort",
    "reverse",
    "concat",
    "fill",
    "at",
    "charAt",
    "charCodeAt",
    "codePointAt",
    "padStart",
    "padEnd",
    "repeat",
    "search",
    "matchAll",
    "exec",
    "toFixed",
    "toISOString",
    "getTime",
    "resolve",
    "reject",
    "all",
    "allSettled",
    "race",
    "any",
    "from",
    "of",
    "isArray",
    "assign",
    "freeze",
    "create",
    "defineProperty",
    "hasOwn",
    "hasOwnProperty",
    "parse",
    "stringify",
    "now",
    "Date",
    "dirname",
    "basename",
    "extname",
    "relative",
    "isAbsolute",
];

pub(super) const JS_STD_QUALIFIERS: &[&str] = &[
    "Array",
    "Object",
    "JSON",
    "Math",
    "Number",
    "String",
    "Boolean",
    "Symbol",
    "BigInt",
    "Date",
    "RegExp",
    "Promise",
    "Reflect",
    "Proxy",
    "Map",
    "Set",
    "WeakMap",
    "WeakSet",
    "Error",
    "TypeError",
    "RangeError",
    "SyntaxError",
    "Uint8Array",
    "Int32Array",
    "Float64Array",
    "ArrayBuffer",
    "DataView",
    "Buffer",
    "console",
    "process",
    "URL",
    "URLSearchParams",
    "Intl",
    "crypto",
    "TextEncoder",
    "TextDecoder",
    "Performance",
    "performance",
    "navigator",
    "window",
    "document",
    "globalThis",
    "fs",
    "path",
    "os",
    "url",
    "util",
    "child_process",
    "Bun",
    "Deno",
];

fn is_cjs_export_target(name_node: tree_sitter::Node<'_>, source_bytes: &[u8]) -> bool {
    let Some(obj) = name_node
        .parent()
        .and_then(|m| m.child_by_field_name("object"))
    else {
        return false;
    };
    if obj.kind() == "identifier" {
        return obj.utf8_text(source_bytes) == Ok("exports");
    }
    if obj.kind() == "member_expression" {
        let is_mod = obj
            .child_by_field_name("object")
            .is_some_and(|o| o.kind() == "identifier" && o.utf8_text(source_bytes) == Ok("module"));
        let is_exp = obj
            .child_by_field_name("property")
            .is_some_and(|p| p.utf8_text(source_bytes) == Ok("exports"));
        return is_mod && is_exp;
    }
    false
}

fn is_prototype_member_target(name_node: tree_sitter::Node<'_>, source_bytes: &[u8]) -> bool {
    name_node
        .parent()
        .and_then(|m| m.child_by_field_name("object"))
        .filter(|o| o.kind() == "member_expression")
        .and_then(|o| o.child_by_field_name("property"))
        .is_some_and(|p| p.utf8_text(source_bytes) == Ok("prototype"))
}

pub(super) fn is_jsx_intrinsic_tag_call(
    name_node: Option<tree_sitter::Node<'_>>,
    name: &str,
) -> bool {
    let is_jsx_tag = name_node
        .and_then(|n| n.parent())
        .is_some_and(|p| matches!(p.kind(), "jsx_opening_element" | "jsx_self_closing_element"));
    is_jsx_tag && name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
}

fn resolve_workspace_package(clean: &str, file_index: &HashMap<&str, u32>) -> Option<u32> {
    let without_scope = clean.strip_prefix('@')?.split_once('/')?.1;
    let (pkg, subpath) = match without_scope.split_once('/') {
        Some((p, sub)) => (p, Some(sub)),
        None => (without_scope, None),
    };
    let short_pkg = pkg.split_once('-').map_or(pkg, |(_, rest)| rest);
    for cand_pkg in [pkg, short_pkg] {
        let base = match subpath {
            Some(sub) => format!("packages/{cand_pkg}/src/{sub}"),
            None => format!("packages/{cand_pkg}/src/index"),
        };
        if let Some(id) = probe_with_suffixes(&base, file_index, WEB_SUFFIXES) {
            return Some(id);
        }
    }
    None
}

pub(super) fn resolve_web_import_target(
    importer_path: &str,
    raw_target: &str,
    file_index: &HashMap<&str, u32>,
) -> Option<u32> {
    let clean = strip_import_quotes(raw_target);
    if clean.starts_with('.') {
        let resolve = |target| resolve_relative_or_exact(importer_path, target, file_index, WEB_SUFFIXES);
        let esm_stem = [".js", ".jsx", ".mjs"].iter().find_map(|ext| clean.strip_suffix(ext));
        return resolve(clean).or_else(|| esm_stem.and_then(resolve));
    }
    if clean.starts_with('@') {
        return resolve_workspace_package(clean, file_index);
    }
    None
}

pub(super) const TS_CONFIG_MANIFESTS: &[&str] = &["tsconfig.json", "jsconfig.json"];

/// `compilerOptions.paths` of a tsconfig/jsconfig as `(pattern, first target)`, targets joined with
/// `baseUrl`; the file may hold comments and trailing commas.
pub(super) fn parse_tsconfig_paths(content: &str) -> Vec<(String, String)> {
    let Ok(config) = serde_json::from_str::<serde_json::Value>(&strip_jsonc(content)) else {
        return Vec::new();
    };
    let options = &config["compilerOptions"];
    let base = options["baseUrl"].as_str().unwrap_or(".");
    let Some(paths) = options["paths"].as_object() else {
        return Vec::new();
    };
    paths
        .iter()
        .filter_map(|(pattern, targets)| {
            let target = targets.as_array()?.first()?.as_str()?;
            Some((pattern.clone(), super::normalize_lexical_path(base, target)))
        })
        .collect()
}

type CharStream<'a> = std::iter::Peekable<std::str::Chars<'a>>;

/// JSON text of a JSONC document: comments and trailing commas outside strings removed.
fn strip_jsonc(content: &str) -> String {
    let mut out = String::with_capacity(content.len());
    let mut chars = content.chars().peekable();
    while let Some(c) = chars.next() {
        match (c, chars.peek()) {
            ('"', _) => copy_json_string(&mut chars, &mut out),
            ('/', Some('/' | '*')) => skip_jsonc_comment(&mut chars),
            ('}' | ']', _) => {
                let kept = out.trim_end().len();
                if out[..kept].ends_with(',') {
                    out.truncate(kept - 1);
                }
                out.push(c);
            }
            _ => out.push(c),
        }
    }
    out
}

/// Copy a string literal whose opening quote was just read, through its closing quote.
fn copy_json_string(chars: &mut CharStream<'_>, out: &mut String) {
    out.push('"');
    while let Some(c) = chars.next() {
        out.push(c);
        match c {
            '\\' => out.extend(chars.next()),
            '"' => return,
            _ => {}
        }
    }
}

/// Skip a `//` or `/* */` comment whose leading `/` was just read.
fn skip_jsonc_comment(chars: &mut CharStream<'_>) {
    if chars.next() == Some('/') {
        while chars.next_if(|&c| c != '\n').is_some() {}
        return;
    }
    while let Some(c) = chars.next() {
        if c == '*' && chars.next_if_eq(&'/').is_some() {
            return;
        }
    }
}

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
        use crate::languages::LintGroup as G;
        &[G::DoWhile, G::Insecure, G::Parens, G::SelfAssign, G::Unreachable, G::Naming]
    }

    fn is_generated_file_name(&self, file_name: &str) -> bool {
        file_name.ends_with(".min.js")
    }
    super::impl_grammar_method!(crate tree_sitter_javascript::LANGUAGE, "tags.scm");

    fn is_read_capture_allowed(&self, node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
        javascript_read_position(node, source)
    }

    fn is_write_capture_allowed(&self, node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
        is_write_target_root(node, source)
    }

    fn read_bindings<'a>(
        &self,
        root: tree_sitter::Node<'_>,
        source: &'a [u8],
    ) -> super::reads::ReadBindings<'a> {
        super::reads::ReadBindings::collect(root, source, |node| {
            javascript_binding_scope(node).map(|scope| scope.byte_range())
        })
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

    fn should_ignore_call(&self, name_node: Option<tree_sitter::Node<'_>>, name: &str) -> bool {
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
}

fn javascript_scope(kind: &str) -> bool {
    matches!(
        kind,
        "function_declaration"
            | "function_expression"
            | "generator_function"
            | "generator_function_declaration"
            | "arrow_function"
            | "method_definition"
            | "statement_block"
            | "catch_clause"
            | "for_statement"
            | "for_in_statement"
    )
}

fn javascript_binding_target(parent: tree_sitter::Node<'_>, node: tree_sitter::Node<'_>) -> bool {
    let field = match parent.kind() {
        "variable_declarator"
        | "function_declaration"
        | "function_expression"
        | "class_declaration"
        | "class"
        | "generator_function"
        | "generator_function_declaration" => "name",
        "required_parameter" | "optional_parameter" => "pattern",
        "catch_clause" | "arrow_function" => "parameter",
        "assignment_pattern" | "pair_pattern" | "for_in_statement" => "left",
        "formal_parameters" => return node.parent() == Some(parent),
        "rest_pattern" | "object_pattern" | "array_pattern" | "import_specifier"
        | "namespace_import" | "import_clause" => return true,
        _ => return false,
    };
    super::reads::in_field(parent, node, field)
}

pub(super) fn javascript_binding_scope(
    node: tree_sitter::Node<'_>,
) -> Option<tree_sitter::Node<'_>> {
    if !matches!(
        node.kind(),
        "identifier" | "shorthand_property_identifier_pattern"
    ) {
        return None;
    }
    let mut ancestor = node.parent();
    while let Some(parent) = ancestor {
        if javascript_binding_target(parent, node) {
            let owner = if matches!(
                parent.kind(),
                "function_declaration" | "generator_function_declaration" | "class_declaration"
            ) {
                parent
            } else {
                node
            };
            return javascript_binding_owner(owner, parent);
        }
        if javascript_scope(parent.kind()) {
            break;
        }
        ancestor = parent.parent();
    }
    None
}

fn javascript_binding_owner<'a>(
    owner: tree_sitter::Node<'a>,
    binding: tree_sitter::Node<'a>,
) -> Option<tree_sitter::Node<'a>> {
    let function_scoped = binding.kind() == "variable_declarator"
        && binding
            .parent()
            .is_some_and(|parent| parent.kind() == "variable_declaration");
    super::reads::enclosing_scope(owner, |kind| {
        javascript_scope(kind)
            && (!function_scoped
                || !matches!(
                    kind,
                    "statement_block" | "catch_clause" | "for_statement" | "for_in_statement"
                ))
    })
}

fn javascript_non_value(parent: tree_sitter::Node<'_>, node: tree_sitter::Node<'_>) -> bool {
    if javascript_binding_target(parent, node) {
        return true;
    }
    match parent.kind() {
        "type_annotation"
        | "type_arguments"
        | "type_parameters"
        | "import_statement"
        | "jsx_opening_element"
        | "jsx_closing_element"
        | "jsx_self_closing_element" => true,
        "call_expression" | "new_expression" => {
            parent.child_by_field_name("function") == Some(node)
                || parent.child_by_field_name("constructor") == Some(node)
        }
        "assignment_expression" | "augmented_assignment_expression" => {
            super::reads::in_field(parent, node, "left")
        }
        "update_expression" => true,
        "pair" => super::reads::in_field(parent, node, "key"),
        _ => false,
    }
}

pub(super) fn javascript_read_position(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    if is_write_target_root(node, source) {
        return false;
    }
    let Some(parent) = node.parent() else {
        return false;
    };
    let expression = matches!(
        parent.kind(),
        "return_statement"
            | "binary_expression"
            | "ternary_expression"
            | "arguments"
            | "array"
            | "object"
            | "pair"
            | "subscript_expression"
            | "member_expression"
            | "expression_statement"
            | "unary_expression"
            | "await_expression"
            | "yield_expression"
            | "spread_element"
            | "template_substitution"
            | "variable_declarator"
            | "assignment_expression"
            | "augmented_assignment_expression"
            | "parenthesized_expression"
            | "if_statement"
            | "while_statement"
            | "for_statement"
            | "for_in_statement"
            | "non_null_expression"
            | "as_expression"
            | "satisfies_expression"
            | "assignment_pattern"
            | "required_parameter"
            | "optional_parameter"
            | "jsx_expression"
            | "throw_statement"
    ) || is_arrow_expression_body(parent, node);
    expression && super::reads::value_position(node, javascript_non_value, javascript_scope)
}

/// Methods that change their receiver in place: a call on a binding is a mutation of the object it holds.
const MUTATING_METHODS: &[&str] = &[
    "add", "clear", "copyWithin", "delete", "fill", "pop", "push", "reverse", "set", "shift", "sort",
    "splice", "unshift",
];

/// True when `node` is the root of a write target: `x` in `x = v`, `x += v`, `x++`, the object of `x.p = v`,
/// `x[i] = v`, or `delete x.p`, or the receiver of an in-place method call `x.push(v)`. Such an identifier is a
/// write, not a read of its value.
pub(super) fn is_write_target_root(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    let mut top = node;
    while let Some(parent) = top.parent() {
        if !matches!(parent.kind(), "member_expression" | "subscript_expression")
            || parent.child_by_field_name("object") != Some(top)
        {
            break;
        }
        top = parent;
    }
    let Some(parent) = top.parent() else {
        return false;
    };
    match parent.kind() {
        "assignment_expression" | "augmented_assignment_expression" => {
            parent.child_by_field_name("left") == Some(top)
        }
        "update_expression" => true,
        "unary_expression" => parent.child(0).is_some_and(|op| op.kind() == "delete"),
        "call_expression" => {
            top.kind() == "member_expression"
                && parent.child_by_field_name("function") == Some(top)
                && top
                    .child_by_field_name("property")
                    .and_then(|property| property.utf8_text(source).ok())
                    .is_some_and(|name| MUTATING_METHODS.contains(&name))
        }
        _ => false,
    }
}

/// The body of an expression-bodied arrow (`() => x`): its value is returned, so the identifier is read.
fn is_arrow_expression_body(parent: tree_sitter::Node<'_>, node: tree_sitter::Node<'_>) -> bool {
    parent.kind() == "arrow_function" && parent.child_by_field_name("body") == Some(node)
}

/// Nesting bound for same-file constant chains, so a cyclic reference cannot recurse forever.
const SCALAR_DEPTH_LIMIT: u8 = 8;

/// True for an initializer whose value is provably a string, number, boolean, bigint or `null`: a literal, a
/// template of scalars, an environment read (`process.env.X`, `import.meta.env.X`), a unary, arithmetic, logical
/// or comparison operator over scalars, or a same-file module `const` that is itself scalar. Objects, arrays,
/// calls and imports are never proven scalar.
fn is_scalar_expression(
    node: tree_sitter::Node<'_>,
    module: tree_sitter::Node<'_>,
    source: &[u8],
    depth: u8,
) -> bool {
    if depth > SCALAR_DEPTH_LIMIT {
        return false;
    }
    let scalar = |child: tree_sitter::Node<'_>| is_scalar_expression(child, module, source, depth + 1);
    match node.kind() {
        "number" | "string" | "true" | "false" | "null" => true,
        "identifier" => node
            .utf8_text(source)
            .is_ok_and(|name| module_const_is_scalar(module, name, source, depth)),
        "template_string" => {
            let mut cursor = node.walk();
            node.named_children(&mut cursor).all(|child| {
                child.kind() != "template_substitution" || child.named_child(0).is_some_and(scalar)
            })
        }
        "unary_expression" => {
            node.child(0).is_some_and(|op| matches!(op.kind(), "-" | "+" | "!" | "~"))
                && node.child_by_field_name("argument").is_some_and(scalar)
        }
        "as_expression" | "satisfies_expression" | "parenthesized_expression" | "non_null_expression" => {
            node.named_child(0).is_some_and(scalar)
        }
        "binary_expression" => {
            let op = node.child_by_field_name("operator").map(|op| op.kind());
            match op {
                Some("===" | "!==" | "==" | "!=" | "<" | ">" | "<=" | ">=") => true,
                Some("+" | "-" | "*" | "/" | "%" | "**" | "??" | "||" | "&&") => {
                    ["left", "right"].into_iter().all(|field| {
                        node.child_by_field_name(field).is_some_and(scalar)
                    })
                }
                _ => false,
            }
        }
        "member_expression" | "subscript_expression" => is_env_read(node, source),
        _ => false,
    }
}

/// `process.env.NAME` / `process.env["NAME"]` (or `import.meta.env`): an environment value, a string or undefined.
fn is_env_read(node: tree_sitter::Node<'_>, source: &[u8]) -> bool {
    node.child_by_field_name("object")
        .and_then(|object| object.utf8_text(source).ok())
        .is_some_and(|text| matches!(text, "process.env" | "import.meta.env"))
}

/// True when the module-level `const name = value` declaration in this file has a scalar value. Only
/// top-level declarations are consulted: an initializer's bare identifier refers to the module scope.
fn module_const_is_scalar(
    module: tree_sitter::Node<'_>,
    name: &str,
    source: &[u8],
    depth: u8,
) -> bool {
    let mut cursor = module.walk();
    for stmt in module.named_children(&mut cursor) {
        let decl = if stmt.kind() == "export_statement" {
            match stmt.child_by_field_name("declaration") {
                Some(decl) => decl,
                None => continue,
            }
        } else {
            stmt
        };
        if decl.kind() != "lexical_declaration" || decl.child(0).is_none_or(|k| k.kind() != "const") {
            continue;
        }
        let mut inner = decl.walk();
        for declarator in decl.named_children(&mut inner) {
            let is_name = declarator
                .child_by_field_name("name")
                .and_then(|n| n.utf8_text(source).ok())
                .is_some_and(|text| text == name);
            if is_name {
                return declarator
                    .child_by_field_name("value")
                    .is_some_and(|value| is_scalar_expression(value, module, source, depth + 1));
            }
        }
    }
    false
}

/// The outermost node of a parse tree: the module a top-level declaration belongs to.
fn module_root(node: tree_sitter::Node<'_>) -> tree_sitter::Node<'_> {
    let mut root = node;
    while let Some(parent) = root.parent() {
        root = parent;
    }
    root
}

/// A module-level `const` bound to an object literal, array literal, or `new` expression: mutable state
/// (`const cache = new Map()`) whatever its case. Other lowercase consts are functions or values, not indexed.
pub(super) fn is_container_const(ctx: &DefCaptureGateContext<'_>) -> bool {
    ctx.name_node
        .and_then(|name| name.parent())
        .filter(|node| node.kind() == "variable_declarator")
        .and_then(|declarator| declarator.child_by_field_name("value"))
        .is_some_and(|value| matches!(value.kind(), "object" | "array" | "new_expression"))
}

/// A TS/JS definition's `(declaration, doc)` span starts. An `export` wrapper supplies the declaration's start
/// (the defining node begins after the keyword). A `/** */` block directly above, with at most one line break
/// between, is its attached doc comment; plain `//` and `/* */` comments are never attached.
pub(super) fn js_definition_span_start(
    def: tree_sitter::Node<'_>,
    source: &[u8],
) -> (usize, usize) {
    let outer = def
        .parent()
        .filter(|parent| parent.kind() == "export_statement")
        .unwrap_or(def);
    let decl = outer.start_byte();
    let doc = outer
        .prev_sibling()
        .filter(|prev| prev.kind() == "comment" && is_attached_jsdoc(*prev, decl, source))
        .map_or(decl, |prev| prev.start_byte());
    (decl, doc)
}

fn is_attached_jsdoc(comment: tree_sitter::Node<'_>, decl: usize, source: &[u8]) -> bool {
    let Some(gap) = source.get(comment.end_byte()..decl) else {
        return false;
    };
    let text = &source[comment.start_byte()..comment.end_byte()];
    text.starts_with(b"/**")
        && text.len() > 4
        && gap.iter().all(u8::is_ascii_whitespace)
        && gap.iter().filter(|&&b| b == b'\n').count() <= 1
}

/// Classifies a captured `Var` binding. A `const` whose value is provably scalar is `Frozen`; any other `const`
/// is `Const`: its binding cannot change, but an object or array it holds can, so it is state only when the
/// indexed files mutate it. `let` and `var` are `Mutable`.
pub(super) fn js_var_binding(ctx: &DefCaptureGateContext<'_>) -> Binding {
    let Some(declarator) = ctx
        .name_node
        .and_then(|name| name.parent())
        .filter(|node| node.kind() == "variable_declarator")
    else {
        return Binding::Mutable;
    };
    let is_const = declarator
        .parent()
        .filter(|decl| decl.kind() == "lexical_declaration")
        .and_then(|decl| decl.child(0))
        .is_some_and(|keyword| keyword.kind() == "const");
    if !is_const {
        return Binding::Mutable;
    }
    let frozen = declarator.child_by_field_name("value").is_some_and(|value| {
        is_scalar_expression(value, module_root(declarator), ctx.source_bytes, 0)
    });
    if frozen { Binding::Frozen } else { Binding::Const }
}

/// A const whose value calls a standard member (`fetch(u).then(f)`, `items.map(f)`) holds data, not a builder.
pub(super) fn is_std_member_call_value(ctx: &DefCaptureGateContext<'_>) -> bool {
    let Some(value) = ctx
        .name_node
        .and_then(|name| name.parent())
        .filter(|node| node.kind() == "variable_declarator")
        .and_then(|declarator| declarator.child_by_field_name("value"))
    else {
        return false;
    };
    value.kind() == "call_expression"
        && value
            .child_by_field_name("function")
            .filter(|callee| callee.kind() == "member_expression")
            .and_then(|callee| callee.child_by_field_name("property"))
            .and_then(|property| property.utf8_text(ctx.source_bytes).ok())
            .is_some_and(|name| JS_STD_MEMBERS.contains(&name))
}

/// JS-family runner: the nearest manifest's vitest/jest, else `require('node:test')` when `node_test` applies.
#[must_use]
pub(crate) fn js_test_run_command(
    path: &str,
    source: &str,
    manifest: Option<&str>,
    node_test: bool,
) -> Option<String> {
    if is_assertion_script(path) {
        // Run by the command its leading comment block documents, not by the manifest's test runner.
        return assertion_header_command(path, source);
    }
    match manifest {
        Some(m) if m.contains("vitest") => Some(format!("npx vitest run {path}")),
        Some(m) if m.contains("jest") => Some(format!("npx jest {path}")),
        _ => (node_test
            && (source.contains("require('node:test')")
                || source.contains("require(\"node:test\")")))
            .then(|| format!("node --test {path}")),
    }
}

/// `x.check.ts` / `.tsx` / `.js`: a standalone assertion script named after the subject it checks.
#[must_use]
pub(crate) fn is_assertion_script(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    [".check.ts", ".check.tsx", ".check.js"]
        .iter()
        .any(|suffix| name.ends_with(suffix))
}

/// Runner prefixes an assertion script's header may document, longest first.
const HEADER_RUNNERS: [&[&str]; 6] = [
    &["pnpm", "dlx", "tsx"],
    &["npx", "tsx"],
    &["tsx"],
    &["node"],
    &["bun"],
    &["deno", "run"],
];

/// The command an assertion script's leading comment block documents for running it, e.g.
/// `//   pnpm dlx tsx src/x.check.ts`. Trusted only when the named file is this script (repo-relative
/// path or basename); the derived command always names the repo-relative path.
fn assertion_header_command(path: &str, source: &str) -> Option<String> {
    let name = path.rsplit('/').next().unwrap_or(path);
    source
        .lines()
        .map(str::trim)
        .take_while(|line| {
            line.is_empty() || line.starts_with("//") || line.starts_with("/*") || line.starts_with('*')
        })
        .find_map(|line| {
            let words: Vec<&str> = line.trim_start_matches(['/', '*']).split_whitespace().collect();
            (0..words.len()).find_map(|at| header_runner_at(&words[at..], path, name))
        })
}

/// The runner command starting at `words[0]` when it names this script as its file argument.
fn header_runner_at(words: &[&str], path: &str, name: &str) -> Option<String> {
    HEADER_RUNNERS.into_iter().find_map(|runner| {
        let target = words.get(runner.len())?;
        (words.starts_with(runner) && (*target == path || *target == name))
            .then(|| format!("{} {path}", runner.join(" ")))
    })
}

/// Partner subject for JS-family files: an assertion script names its subject as `x.check`, else the
/// universal test-naming conventions apply.
pub(crate) fn js_test_partner_subject(file_stem: &str, in_test_dir: bool) -> Option<&str> {
    file_stem
        .strip_suffix(".check")
        .filter(|subject| !subject.is_empty())
        .or_else(|| universal_test_partner_subject(file_stem, in_test_dir))
}

