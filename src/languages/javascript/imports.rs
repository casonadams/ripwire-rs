use crate::languages::{probe_with_suffixes, resolve_relative_or_exact, strip_import_quotes};
use std::collections::HashMap;

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

/// The module part of an import reference: `"p as X"` names module `p` bound to `X`.
fn module_part(raw: &str) -> &str {
    raw.split_once(" as ").map_or(raw, |(module, _)| module)
}

pub(crate) fn resolve_web_import_target(
    importer_path: &str,
    raw_target: &str,
    file_index: &HashMap<&str, u32>,
) -> Option<u32> {
    let clean = strip_import_quotes(module_part(raw_target));
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

/// The module paths of `import("path")` calls: a dynamic import names no binding, so the importer reaches
/// whatever the module exports (`lazy(() => import("./Panel"))` loads its default export).
pub(in crate::languages) fn dynamic_import_paths(root: tree_sitter::Node<'_>, source: &[u8]) -> Vec<String> {
    let mut paths = Vec::new();
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if node.kind() == "call_expression"
            && node
                .child_by_field_name("function")
                .is_some_and(|function| function.kind() == "import")
            && let Some(module) = node
                .child_by_field_name("arguments")
                .and_then(|args| args.named_child(0))
                .filter(|module| module.kind() == "string")
            && let Some(text) = node_text(module, source)
        {
            paths.push(strip_import_quotes(&text).to_string());
        }
        let mut cursor = node.walk();
        stack.extend(node.children(&mut cursor));
    }
    paths.sort_unstable();
    paths.dedup();
    paths
}

/// Import references for `import` statements and CommonJS `require` calls, each named with the local names it
/// binds (`"p as X"`), so a name the file imports resolves to the module the import names.
pub(super) fn js_import_refs(
    root: tree_sitter::Node<'_>,
    source: &[u8],
    refs: &mut Vec<crate::ingest::RawRef>,
) {
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        match node.kind() {
            "import_statement" => push_import_statement(node, source, refs),
            "call_expression" => push_require(node, source, refs),
            _ => {}
        }
        let mut cursor = node.walk();
        let children: Vec<_> = node.children(&mut cursor).collect();
        stack.extend(children.into_iter().rev());
    }
}

fn push_import_statement(
    stmt: tree_sitter::Node<'_>,
    source: &[u8],
    refs: &mut Vec<crate::ingest::RawRef>,
) {
    let Some(module) = stmt.child_by_field_name("source") else {
        return;
    };
    let names = es_bound_names(stmt, source);
    push_import_refs(module, source, names, refs);
}

/// The local names an ES import clause binds: default, namespace, and each named specifier's alias or name.
fn es_bound_names(stmt: tree_sitter::Node<'_>, source: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    for clause in children(stmt).filter(|c| c.kind() == "import_clause") {
        for part in children(clause) {
            match part.kind() {
                "identifier" => names.extend(node_text(part, source)),
                "namespace_import" => {
                    names.extend(
                        children(part)
                            .filter(|c| c.kind() == "identifier")
                            .filter_map(|c| node_text(c, source)),
                    );
                }
                "named_imports" => {
                    for spec in children(part).filter(|c| c.kind() == "import_specifier") {
                        let local = spec
                            .child_by_field_name("alias")
                            .or_else(|| spec.child_by_field_name("name"));
                        names.extend(local.and_then(|n| node_text(n, source)));
                    }
                }
                _ => {}
            }
        }
    }
    names
}

fn push_require(call: tree_sitter::Node<'_>, source: &[u8], refs: &mut Vec<crate::ingest::RawRef>) {
    let is_require = call.child_by_field_name("function").is_some_and(|f| {
        f.kind() == "identifier" && node_text(f, source).as_deref() == Some("require")
    });
    if !is_require {
        return;
    }
    let Some(module) = call
        .child_by_field_name("arguments")
        .and_then(|args| args.named_child(0))
        .filter(|m| m.kind() == "string")
    else {
        return;
    };
    let names = require_bound_names(call, source);
    push_import_refs(module, source, names, refs);
}

/// The local names a `require` call binds: `const X = require(p)`, `const X = require(p).default`, and the
/// names of a destructuring pattern `const {a, b: c} = require(p)`.
fn require_bound_names(call: tree_sitter::Node<'_>, source: &[u8]) -> Vec<String> {
    let mut site = call;
    if let Some(member) = call.parent().filter(|p| {
        p.kind() == "member_expression" && p.child_by_field_name("object") == Some(call)
    }) {
        site = member;
    }
    let Some(decl) = site.parent().filter(|d| {
        d.kind() == "variable_declarator" && d.child_by_field_name("value") == Some(site)
    }) else {
        return Vec::new();
    };
    let Some(name) = decl.child_by_field_name("name") else {
        return Vec::new();
    };
    match name.kind() {
        "identifier" => node_text(name, source).into_iter().collect(),
        "object_pattern" => destructured_names(name, source),
        _ => Vec::new(),
    }
}

fn destructured_names(pattern: tree_sitter::Node<'_>, source: &[u8]) -> Vec<String> {
    children(pattern)
        .filter_map(|part| match part.kind() {
            "shorthand_property_identifier_pattern" => node_text(part, source),
            "pair_pattern" => part
                .child_by_field_name("value")
                .filter(|v| v.kind() == "identifier")
                .and_then(|v| node_text(v, source)),
            _ => None,
        })
        .collect()
}

/// One import reference per bound name, or a single plain reference when the import binds nothing.
fn push_import_refs(
    module: tree_sitter::Node<'_>,
    source: &[u8],
    names: Vec<String>,
    refs: &mut Vec<crate::ingest::RawRef>,
) {
    let Some(text) = node_text(module, source) else {
        return;
    };
    let path = strip_import_quotes(&text);
    let line = module.start_position().row as u32 + 1;
    let start_byte = module.start_byte();
    if names.is_empty() {
        refs.push(import_ref(path.to_string(), line, start_byte));
    }
    for name in names {
        refs.push(import_ref(format!("{path} as {name}"), line, start_byte));
    }
}

fn import_ref(callee_name: String, line: u32, start_byte: usize) -> crate::ingest::RawRef {
    crate::ingest::RawRef::positioned(callee_name, crate::domain::RefRole::Import, line, start_byte)
}

fn children(node: tree_sitter::Node<'_>) -> impl Iterator<Item = tree_sitter::Node<'_>> {
    let mut cursor = node.walk();
    let all: Vec<_> = node.children(&mut cursor).collect();
    all.into_iter()
}

fn node_text(node: tree_sitter::Node<'_>, source: &[u8]) -> Option<String> {
    node.utf8_text(source).ok().map(str::to_string)
}
