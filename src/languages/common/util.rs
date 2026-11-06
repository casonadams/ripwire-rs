//! Path, import-resolution, naming and visibility helpers shared by language plugins.
use super::SyntaxKinds;
use crate::domain::Lang;
use crate::languages::ALL_LANGS;
use std::collections::HashMap;
use std::path::Path;

pub(crate) fn is_non_entrypoint_path(rel_path: &str) -> bool {
    if crate::domain::test_path::is_test_or_bench_path(rel_path) {
        return true;
    }
    Path::new(rel_path).components().any(|c| {
        c.as_os_str().to_str().is_some_and(|s| {
            matches!(
                s.to_ascii_lowercase().as_str(),
                "example" | "examples" | "doc" | "docs"
            )
        })
    })
}

/// The ancestors of `node` from its nearest to `root`. `Node::parent` re-descends from the root on every call,
/// so a loop over it costs quadratic time in depth; one descent from `root` yields the whole chain.
pub(crate) fn ancestors<'t>(
    root: tree_sitter::Node<'t>,
    node: tree_sitter::Node<'t>,
) -> Vec<tree_sitter::Node<'t>> {
    if root == node {
        return Vec::new();
    }
    let mut chain = vec![root];
    while let Some(next) = chain.last().and_then(|cur| cur.child_with_descendant(node)) {
        if next == node {
            break;
        }
        chain.push(next);
    }
    chain.reverse();
    chain
}

pub(crate) fn find_enclosing_definition_node(
    node: tree_sitter::Node<'_>,
) -> Option<tree_sitter::Node<'_>> {
    let mut curr = node;
    while let Some(parent) = curr.parent() {
        if matches!(
            parent.kind(),
            "function_definition"
                | "method_definition"
                | "constructor_definition"
                | "destructor_definition"
        ) {
            return Some(parent);
        }
        if matches!(
            parent.kind(),
            "compound_statement"
                | "statement_block"
                | "block"
                | "declaration_list"
                | "field_declaration_list"
                | "class_body"
                | "ERROR"
        ) {
            return None;
        }
        curr = parent;
    }
    None
}

pub(crate) fn check_leading_underscore_private(
    def_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
    kinds: &SyntaxKinds,
) -> bool {
    let node = crate::gate::contract::unwrap_signature_node(def_node, kinds);
    let is_private = node
        .child_by_field_name("name")
        .and_then(|n| n.utf8_text(source_bytes).ok())
        .is_some_and(|s| s.starts_with('_'));
    !is_private
}

fn has_visibility_modifier(child: tree_sitter::Node<'_>, source_bytes: &[u8]) -> bool {
    let kind = child.kind();
    if (kind == "visibility_modifier" || kind == "accessibility_modifier")
        && let Ok(text) = child.utf8_text(source_bytes)
    {
        return text.contains("pub") || text.contains("public");
    }
    false
}

pub(crate) fn has_any_visibility_modifier(
    def_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
) -> bool {
    for i in 0..def_node.child_count() {
        if let Some(child) = def_node.child(i)
            && has_visibility_modifier(child, source_bytes)
        {
            return true;
        }
    }
    false
}

#[must_use]
pub fn strip_import_quotes(raw: &str) -> &str {
    raw.trim()
        .trim_matches(|c: char| matches!(c, '"' | '\'' | '`' | ';'))
        .trim()
}

#[must_use]
pub(crate) fn strip_camel_test_suffix(file_stem: &str) -> Option<&str> {
    file_stem
        .strip_suffix("Test")
        .filter(|s| s.ends_with(|c: char| c.is_ascii_alphanumeric()))
}

/// Filename suffixes that name a partner test after its subject (`x_test.py`, `x.spec.ts`).
const PARTNER_SUFFIXES: [&str; 4] = ["_test", "_spec", ".test", ".spec"];

/// True when some directory component of `p`'s parent satisfies `pred`, compared lowercase.
pub(crate) fn has_dir_component(p: &Path, pred: impl Fn(&str) -> bool) -> bool {
    p.parent()
        .into_iter()
        .flat_map(Path::components)
        .filter_map(|c| c.as_os_str().to_str())
        .any(|d| pred(&d.to_ascii_lowercase()))
}

/// The subject a test file stem names by the universal conventions: `test_x`, `x_test`, `x_spec`,
/// `x.test`, `x.spec`, and `XTest` when the file sits under a `test/` or `tests/` directory.
#[must_use]
pub(crate) fn universal_test_partner_subject(file_stem: &str, in_test_dir: bool) -> Option<&str> {
    file_stem
        .strip_prefix("test_")
        .or_else(|| {
            PARTNER_SUFFIXES
                .iter()
                .find_map(|s| file_stem.strip_suffix(s))
        })
        .filter(|s| !s.is_empty())
        .or_else(|| {
            in_test_dir
                .then(|| strip_camel_test_suffix(file_stem))
                .flatten()
        })
}

#[must_use]
pub(crate) fn is_screaming_snake(name: &str) -> bool {
    let bytes = name.as_bytes();
    bytes.len() >= 2
        && bytes[0].is_ascii_uppercase()
        && bytes
            .iter()
            .all(|&b| b.is_ascii_uppercase() || b.is_ascii_digit() || b == b'_')
}

#[must_use]
pub(crate) fn is_plain_type_name(text: &str) -> bool {
    !text.is_empty() && text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// A type written as a dot-separated path of plain names (`Outer.Inner`).
pub(crate) fn is_type_path(text: &str) -> bool {
    text.split('.').all(is_plain_type_name)
}

#[must_use]
pub fn normalize_lexical_path(base_dir: &str, rel: &str) -> String {
    let combined = if base_dir.is_empty() {
        rel.to_string()
    } else {
        format!("{base_dir}/{rel}")
    };
    let mut parts: Vec<&str> = Vec::new();
    for seg in combined.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

pub(crate) fn probe_with_suffixes(
    candidate_base: &str,
    file_index: &HashMap<&str, u32>,
    suffixes: &[&str],
) -> Option<u32> {
    if let Some(&id) = file_index.get(candidate_base) {
        return Some(id);
    }
    let mut key = String::with_capacity(candidate_base.len() + 8);
    for sfx in suffixes {
        key.clear();
        key.push_str(candidate_base);
        key.push_str(sfx);
        if let Some(&id) = file_index.get(key.as_str()) {
            return Some(id);
        }
    }
    None
}

pub(crate) fn needs_lexical_normalization(path: &str) -> bool {
    path.split('/').any(|seg| matches!(seg, "" | "." | ".."))
}

pub(crate) fn resolve_relative_or_exact(
    importer_path: &str,
    clean_target: &str,
    file_index: &HashMap<&str, u32>,
    suffixes: &[&str],
) -> Option<u32> {
    let base_dir = Path::new(importer_path)
        .parent()
        .and_then(|p| p.to_str())
        .unwrap_or("");
    let rel_cand = normalize_lexical_path(base_dir, clean_target);
    if let Some(id) = probe_with_suffixes(&rel_cand, file_index, suffixes) {
        return Some(id);
    }
    if !clean_target.starts_with('.') {
        let root_cand = normalize_lexical_path("", clean_target);
        return probe_with_suffixes(&root_cand, file_index, suffixes);
    }
    None
}

/// The aliases a manifest at `rel_path` declares, when some language reads that manifest.
#[must_use]
pub fn import_aliases_in(rel_path: &str, content: &str) -> Vec<crate::domain::ImportAlias> {
    let name = rel_path.rsplit('/').next().unwrap_or(rel_path);
    let Some(lang) = ALL_LANGS
        .iter()
        .find(|l| l.semantics().import_alias_manifests().contains(&name))
    else {
        return Vec::new();
    };
    let dir = rel_path.rsplit_once('/').map_or("", |(d, _)| d);
    lang.semantics()
        .parse_import_aliases(content)
        .into_iter()
        .map(|(pattern, target)| crate::domain::ImportAlias {
            dir: dir.to_string(),
            pattern,
            target: normalize_lexical_path(dir, &target),
        })
        .collect()
}

/// Resolve `raw` imported from `importer_path`: the language's own rules, then the aliases of the
/// nearest manifest above the importer when the language reads such manifests.
#[must_use]
pub fn resolve_import(
    lang: Lang,
    importer_path: &str,
    raw: &str,
    file_index: &HashMap<&str, u32>,
    aliases: &[crate::domain::ImportAlias],
) -> Option<u32> {
    let semantics = lang.semantics();
    if let Some(id) = semantics.resolve_import_target(importer_path, raw, file_index) {
        return Some(id);
    }
    if semantics.import_alias_manifests().is_empty() {
        return None;
    }
    let applies = |a: &&crate::domain::ImportAlias| {
        a.dir.is_empty() || importer_path.starts_with(&format!("{}/", a.dir))
    };
    let nearest = aliases.iter().filter(applies).map(|a| a.dir.len()).max()?;
    let spec = strip_import_quotes(raw);
    aliases
        .iter()
        .filter(|a| a.dir.len() == nearest && applies(a))
        .filter_map(|a| match a.pattern.split_once('*') {
            Some((pre, post)) => spec
                .strip_prefix(pre)
                .and_then(|rest| rest.strip_suffix(post))
                .map(|mid| a.target.replacen('*', mid, 1)),
            None => (spec == a.pattern).then(|| a.target.clone()),
        })
        .find_map(|path| semantics.resolve_import_target("", &format!("./{path}"), file_index))
}
