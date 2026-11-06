use crate::domain::Lang;
use crate::ingest::LanguageRegistry;
use crate::languages::SyntaxKinds;
use std::path::{Path, PathBuf};
use streaming_iterator::StreamingIterator;

/// Function signature contract defining parameter count and public visibility.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignatureContract {
    pub name: String,
    pub params: u16,
    pub required_params: u16,
    pub is_public: bool,
}

/// Parameters a call may bind. `max` counts every positional slot (a variadic counts `VARIADIC_PARAMS`);
/// `required` counts only slots without a default value.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ParamArity {
    pub max: u16,
    pub required: u16,
}

/// A `max` at or above this bound is open-ended: the definition takes a variadic parameter.
pub const VARIADIC_PARAMS: u16 = 64;

impl ParamArity {
    #[must_use]
    pub const fn fixed(n: u16) -> Self {
        Self {
            max: n,
            required: n,
        }
    }

    /// Whether a call passing `args` arguments binds every required slot and no extra one.
    #[must_use]
    pub fn accepts(self, args: u16) -> bool {
        args >= self.required && (self.max >= VARIADIC_PARAMS || args <= self.max)
    }

    /// The arity a `recv.method(args)` call sees: the receiver is not written as an argument.
    #[must_use]
    pub fn without_receiver(self) -> Self {
        Self {
            max: self.max.saturating_sub(1),
            required: self.required.saturating_sub(1),
        }
    }
}

/// Comparison result between current AST signature and baseline (git HEAD).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContractStatus {
    Unchanged,
    ContractChange {
        param_changed: bool,
        public_changed: bool,
        was_params: u16,
        now_params: u16,
        was_public: bool,
        now_public: bool,
    },
    NewSymbol,
    NoBaseline,
}

impl std::fmt::Display for ContractStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = match self {
            Self::Unchanged => "unchanged",
            Self::ContractChange { .. } => "contract-change",
            Self::NewSymbol => "new-symbol",
            Self::NoBaseline => "no-baseline",
        };
        write!(f, "{s}")
    }
}

/// Compare working tree signature contract against a baseline.
#[must_use]
pub fn compare_contracts(
    now: &SignatureContract,
    was: Option<&SignatureContract>,
) -> ContractStatus {
    let Some(baseline) = was else {
        return ContractStatus::NewSymbol;
    };

    let param_changed =
        now.params != baseline.params || now.required_params != baseline.required_params;
    let public_changed = now.is_public != baseline.is_public;

    if !param_changed && !public_changed {
        ContractStatus::Unchanged
    } else {
        ContractStatus::ContractChange {
            param_changed,
            public_changed,
            was_params: baseline.params,
            now_params: now.params,
            was_public: baseline.is_public,
            now_public: now.is_public,
        }
    }
}

fn tree_entry_blob(
    tree: &gix::Tree<'_>,
    path: impl AsRef<Path>,
) -> Result<Option<Vec<u8>>, String> {
    let Some(object) = crate::git::repo::object_at(tree, path)? else {
        return Ok(None);
    };
    Ok(Some(object.data.to_vec()))
}

fn resolve_repo_relative_path(repo_path: &Path, workdir: &Path, rel_path: &str) -> Option<PathBuf> {
    let full_target = repo_path.canonicalize().ok()?.join(rel_path);
    let canon_workdir = workdir.canonicalize().ok()?;
    let repo_rel = full_target.strip_prefix(&canon_workdir).ok()?;
    Some(repo_rel.to_path_buf())
}

pub(crate) fn read_head_blob_bytes(
    repo_path: &Path,
    rel_path: &str,
) -> Result<Option<Vec<u8>>, String> {
    let Ok(repo) = gix::discover(repo_path) else {
        return Ok(None);
    };
    let tree = repo
        .head_commit()
        .map_err(|e| format!("HEAD: {e}"))?
        .tree()
        .map_err(|e| format!("HEAD tree: {e}"))?;
    if let Some(bytes) = tree_entry_blob(&tree, rel_path)? {
        return Ok(Some(bytes));
    }
    let Some(workdir) = repo.workdir() else {
        return Ok(None);
    };
    match resolve_repo_relative_path(repo_path, workdir, rel_path) {
        Some(repo_rel) => tree_entry_blob(&tree, repo_rel),
        None => Ok(None),
    }
}

fn resolve_head_visibility(
    name: Option<&str>,
    def_node: Option<tree_sitter::Node<'_>>,
    bytes: &[u8],
    lang: Lang,
) -> bool {
    if lang.semantics().visibility_mode() == crate::languages::VisibilityMode::ByNameConvention {
        return name.is_none_or(|n| is_symbol_name_public(n, lang));
    }
    def_node.is_none_or(|dn| check_node_visibility(dn, bytes, lang))
}

pub fn parse_head_signature_match(
    m: &tree_sitter::QueryMatch<'_, '_>,
    query: &tree_sitter::Query,
    bytes: &[u8],
    lang: Lang,
) -> Option<(String, ParamArity, bool)> {
    let mut name = None;
    let mut arity = ParamArity::default();
    let mut def_node = None;

    for capture in m.captures {
        let cap_name = query.capture_names()[capture.index as usize];
        let node = capture.node;
        if cap_name == "name" {
            if let Ok(text) = node.utf8_text(bytes) {
                name = Some(text.to_string());
            }
        } else if cap_name.starts_with("definition.") {
            arity = extract_param_arity(node, lang.semantics().syntax_kinds());
            def_node = Some(node);
        }
    }
    let dn = def_node?;
    let n = name?;
    let is_pub = resolve_head_visibility(Some(&n), Some(dn), bytes, lang);
    Some((n, arity, is_pub))
}

/// Parse a symbol's contract from source bytes in memory.
pub fn parse_symbol_contract_from_bytes(
    bytes: &[u8],
    sym_name: &str,
    lang: Lang,
) -> Result<Option<SignatureContract>, String> {
    let registry = LanguageRegistry::new().map_err(|e| e.to_string())?;
    let Some((ts_lang, query)) = registry.get(lang) else {
        return Ok(None);
    };

    let mut parser = tree_sitter::Parser::new();
    if parser.set_language(ts_lang).is_err() {
        return Ok(None);
    }

    let Some(tree) = parser.parse(bytes, None) else {
        return Ok(None);
    };

    let mut cursor = tree_sitter::QueryCursor::new();
    let mut matches = cursor.matches(query, tree.root_node(), bytes);

    while let Some(m) = matches.next() {
        if let Some((n, arity, is_public)) = parse_head_signature_match(m, query, bytes, lang)
            && n == sym_name
        {
            return Ok(Some(SignatureContract {
                name: n,
                params: arity.max,
                required_params: arity.required,
                is_public,
            }));
        }
    }

    Ok(None)
}

/// Load a symbol's contract from the git HEAD commit.
pub fn load_head_symbol_contract<P: AsRef<Path>>(
    repo_path: P,
    rel_path: &str,
    sym_name: &str,
    lang: Lang,
) -> Result<Option<SignatureContract>, String> {
    let Some(bytes) = read_head_blob_bytes(repo_path.as_ref(), rel_path)? else {
        return Ok(None);
    };
    parse_symbol_contract_from_bytes(&bytes, sym_name, lang)
}

fn is_variadic_param_kind(kind: &str) -> bool {
    ["rest", "variadic", "splat", "spread"]
        .iter()
        .any(|marker| kind.contains(marker))
}

/// Variadic: Python's `list_splat_pattern`, or TypeScript's `required_parameter` wrapping `...xs`.
fn is_variadic_param(param: tree_sitter::Node<'_>) -> bool {
    is_variadic_param_kind(param.kind())
        || param
            .named_child(0)
            .is_some_and(|child| is_variadic_param_kind(child.kind()))
}

fn is_optional_param_kind(kind: &str) -> bool {
    kind.contains("optional_parameter") || kind.contains("default_parameter")
}

/// Separators (`*`, `/`) and Ruby's `&block` take no positional argument.
fn is_arity_free_param_kind(kind: &str) -> bool {
    kind.contains("separator") || kind.contains("block_parameter")
}

/// A default value (`x=1`, Ruby `k: 1`, C# `int x = 1`, PHP `$x = 1`) makes a parameter optional.
fn has_default_value(param: tree_sitter::Node<'_>) -> bool {
    param
        .child_by_field_name("value")
        .or_else(|| param.child_by_field_name("default_value"))
        .is_some()
}

fn count_single_param(
    param: tree_sitter::Node<'_>,
    strict: bool,
    kinds: &SyntaxKinds,
) -> ParamArity {
    let kind = param.kind();
    if is_variadic_param(param) {
        return ParamArity {
            max: VARIADIC_PARAMS,
            required: 0,
        };
    }
    if is_arity_free_param_kind(kind) {
        return ParamArity::default();
    }
    if kinds.optional_param_groups.contains(&kind) {
        let mut cursor = param.walk();
        let max = param
            .named_children(&mut cursor)
            .filter(|p| p.kind().contains("param"))
            .count() as u16;
        return ParamArity { max, required: 0 };
    }
    let mut cursor = param.walk();
    let names = param.children_by_field_name("name", &mut cursor).count();
    if names > 1 {
        // Go's `a, b string` is one declaration node naming several parameters.
        return ParamArity::fixed(names as u16);
    }
    let count = u16::from(if strict {
        kind == "parameter"
    } else {
        !kind.contains("comment")
    });
    let optional = is_optional_param_kind(kind) || has_default_value(param);
    ParamArity {
        max: count,
        required: if optional { 0 } else { count },
    }
}

fn count_param_children(list: tree_sitter::Node<'_>, kinds: &SyntaxKinds) -> ParamArity {
    let strict = kinds.strict_param_lists.contains(&list.kind());
    let mut cursor = list.walk();
    list.named_children(&mut cursor)
        .map(|p| count_single_param(p, strict, kinds))
        .fold(ParamArity::default(), |acc, p| ParamArity {
            max: acc.max + p.max,
            required: acc.required + p.required,
        })
}

fn find_callable_descendant<'a>(
    node: tree_sitter::Node<'a>,
    depth: u8,
    kinds: &SyntaxKinds,
) -> Option<tree_sitter::Node<'a>> {
    if depth == 0 {
        return None;
    }
    let mut cursor = node.walk();
    node.named_children(&mut cursor).find_map(|ch| {
        if kinds.callables.contains(&ch.kind()) {
            Some(ch)
        } else if kinds.callable_wrappers.contains(&ch.kind()) {
            find_callable_descendant(ch, depth - 1, kinds)
        } else {
            None
        }
    })
}

#[must_use]
pub fn unwrap_signature_node<'a>(
    def_node: tree_sitter::Node<'a>,
    kinds: &SyntaxKinds,
) -> tree_sitter::Node<'a> {
    if kinds.signature_holders.contains(&def_node.kind()) {
        let mut cursor = def_node.walk();
        if let Some(signature) = def_node
            .named_children(&mut cursor)
            .find(|ch| ch.kind().contains("signature"))
        {
            return signature;
        }
    }
    find_callable_descendant(def_node, 4, kinds).unwrap_or(def_node)
}

fn is_value_parameter_list_kind(kind: &str) -> bool {
    kind.contains("parameter") && !kind.contains("type_parameter") && !kind.starts_with("template_")
}

#[must_use]
pub fn extract_param_arity(def_node: tree_sitter::Node<'_>, kinds: &SyntaxKinds) -> ParamArity {
    let node = unwrap_signature_node(def_node, kinds);
    let mut cursor = node.walk();
    if let Some(list) = node
        .children(&mut cursor)
        .find(|child| is_value_parameter_list_kind(child.kind()))
    {
        return count_param_children(list, kinds);
    }
    let single_param =
        kinds.callables.contains(&node.kind()) && node.child_by_field_name("parameter").is_some();
    ParamArity::fixed(u16::from(single_param))
}

#[must_use]
pub fn is_symbol_name_public(sym_name: &str, lang: Lang) -> bool {
    lang.semantics().is_symbol_name_public(sym_name)
}

#[must_use]
pub fn check_node_visibility(
    def_node: tree_sitter::Node<'_>,
    source_bytes: &[u8],
    lang: Lang,
) -> bool {
    lang.semantics()
        .check_node_visibility(def_node, source_bytes)
}
