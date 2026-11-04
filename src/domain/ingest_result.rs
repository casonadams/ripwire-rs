use super::{Interner, Lang, NodeId, Reference, SymKind, Symbol};
use lasso::Spur;
use serde::{Deserialize, Serialize};

/// Metadata for a crawled file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileInfo {
    pub id: u32,
    pub path: Spur,
    pub lang: Lang,
    pub line_count: u32,
    pub byte_size: u64,
}

/// A module-path alias a project manifest declares (e.g. tsconfig `paths`): in files under `dir`, a
/// specifier matching `pattern` names the root-relative `target`, where `*` takes the matched rest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ImportAlias {
    pub dir: String,
    pub pattern: String,
    pub target: String,
}

/// A definition a binding framework in file `file_id` exposes to `caller`-language code as `exported`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ForeignBinding {
    pub file_id: u32,
    pub exported: Spur,
    pub target: Spur,
    pub target_scope: Option<Spur>,
    pub caller: Lang,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallableBinding {
    pub file_id: u32,
    pub from_symbol: NodeId,
    pub start_byte: u32,
    pub owner: Spur,
    pub field: Spur,
    pub target: Option<Spur>,
    pub target_qualifier: Option<Spur>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Implementation {
    pub file_id: u32,
    pub subject: Spur,
    pub trait_name: Option<Spur>,
    pub trait_qualifier: Option<Spur>,
    pub subject_bound: Option<Spur>,
    pub generic_subject: bool,
    pub start_byte: u32,
    pub end_byte: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ScipTarget {
    Def(NodeId),
    External,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScipSite {
    pub file_id: u32,
    pub line: u32,
    pub callee: Spur,
    pub target: ScipTarget,
}

/// Ingest output: the contract between AST ingestion and graph construction.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct IngestResult {
    pub files: Vec<FileInfo>,
    pub symbols: Vec<Symbol>,
    pub references: Vec<Reference>,
    #[serde(default)]
    pub import_aliases: Vec<ImportAlias>,
    #[serde(default)]
    pub foreign_bindings: Vec<ForeignBinding>,
    #[serde(default)]
    pub scip_sites: Vec<ScipSite>,
    #[serde(default)]
    pub callable_bindings: Vec<CallableBinding>,
    #[serde(default)]
    pub implementations: Vec<Implementation>,
    /// Crawled files whose read failed (permission, or removed mid-run). They are indexed as empty
    /// files, so every answer over them is a floor.
    #[serde(default)]
    pub unreadable: usize,
}

impl IngestResult {
    pub fn new() -> Self {
        Self::default()
    }

    /// Indexed definitions, excluding synthetic `ModuleScope` owners.
    pub fn symbol_count(&self) -> usize {
        self.symbols
            .iter()
            .filter(|s| s.kind != SymKind::ModuleScope)
            .count()
    }

    pub fn reference_count(&self) -> usize {
        self.references.len()
    }

    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    #[inline]
    #[must_use]
    pub fn sym(&self, id: NodeId) -> Option<&Symbol> {
        self.symbols.get(id as usize)
    }

    #[inline]
    #[must_use]
    pub fn file(&self, file_id: u32) -> Option<&FileInfo> {
        self.files.get(file_id as usize)
    }

    #[inline]
    #[must_use]
    pub fn file_path<'a>(&self, file_id: u32, interner: &'a Interner) -> &'a str {
        self.file(file_id)
            .and_then(|f| interner.try_resolve(&f.path))
            .unwrap_or("")
    }

    #[inline]
    #[must_use]
    pub fn sym_file_path<'a>(&self, sym: &Symbol, interner: &'a Interner) -> &'a str {
        self.file_path(sym.file_id, interner)
    }

    #[inline]
    #[must_use]
    pub fn sym_name<'a>(&self, sym: &Symbol, interner: &'a Interner) -> &'a str {
        interner.try_resolve(&sym.name).unwrap_or("")
    }
}

impl std::ops::Index<NodeId> for IngestResult {
    type Output = Symbol;

    #[inline]
    fn index(&self, id: NodeId) -> &Self::Output {
        &self.symbols[id as usize]
    }
}
