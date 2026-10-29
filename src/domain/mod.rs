pub mod corpus;

pub use corpus::{CorpusView, read_rel_bytes, read_rel_string};

use lasso::{Key, Spur, ThreadedRodeo};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// 32-bit index into symbol tables. Not a pointer.
pub type NodeId = u32;

/// Sentinel value indicating no node.
pub const NO_NODE: NodeId = u32::MAX;

/// Name of the synthetic per-file owner of references outside every definition (`SymKind::ModuleScope`).
pub const MODULE_SCOPE_NAME: &str = "<file-scope>";

/// Thread-safe string interner backed by `lasso::ThreadedRodeo`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Interner(Arc<ThreadedRodeo>);

impl Interner {
    pub fn new() -> Self {
        Self::default()
    }
    #[inline]
    pub fn intern(&self, s: &str) -> Spur {
        self.0.get_or_intern(s)
    }
    #[inline]
    pub fn get(&self, s: &str) -> Option<Spur> {
        self.0.get(s)
    }
    #[inline]
    pub fn resolve<'a>(&'a self, spur: &Spur) -> &'a str {
        self.0.resolve(spur)
    }
    #[inline]
    pub fn try_resolve<'a>(&'a self, spur: &Spur) -> Option<&'a str> {
        self.0.try_resolve(spur)
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    #[must_use]
    pub fn all_strings(&self) -> Vec<String> {
        let mut out = Vec::with_capacity(self.0.len());
        for i in 0..self.0.len() {
            if let Some(spur) = Spur::try_from_usize(i)
                && let Some(s) = self.0.try_resolve(&spur)
            {
                out.push(s.to_string());
            }
        }
        out
    }
    #[must_use]
    pub fn from_strings(strings: &[String]) -> Self {
        let rodeo = ThreadedRodeo::new();
        for s in strings {
            rodeo.get_or_intern(s);
        }
        Self(Arc::new(rodeo))
    }
}

impl std::ops::Index<Spur> for Interner {
    type Output = str;

    #[inline]
    fn index(&self, spur: Spur) -> &Self::Output {
        self.resolve(&spur)
    }
}

impl std::ops::Index<&Spur> for Interner {
    type Output = str;

    #[inline]
    fn index(&self, spur: &Spur) -> &Self::Output {
        self.resolve(spur)
    }
}

/// Symbol kind corresponding to the terse XML attribute `t="..."`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SymKind {
    Function = 0,
    Method,
    Class,
    Struct,
    Interface,
    Var,
    Section,
    Macro,
    Field,
    Other,
    ModuleScope,
}

impl SymKind {
    #[inline]
    pub const fn tag(self) -> &'static str {
        const TAGS: [&str; 11] = [
            "fn", "method", "cls", "struct", "iface", "var", "sec", "macro", "field", "other",
            "modscope",
        ];
        TAGS[self as usize]
    }
    #[inline]
    #[must_use]
    pub const fn is_callable(self) -> bool {
        matches!(self, Self::Function | Self::Method)
    }

    #[inline]
    #[must_use]
    pub const fn is_type_container(self) -> bool {
        matches!(self, Self::Class | Self::Struct | Self::Interface)
    }

    #[inline]
    #[must_use]
    pub const fn is_code_symbol(self) -> bool {
        !matches!(self, Self::Section | Self::ModuleScope)
    }
}

impl std::fmt::Display for SymKind {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.tag())
    }
}

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RefRole {
    Call = 0,
    Read,
    Write,
    Import,
    Extends,
    Macro,
    Type,
    Value,
    Through,
    /// A member read that may run an accessor (`x.prop` of a getter or `@property`).
    Access,
}

impl RefRole {
    #[inline]
    pub const fn tag(self) -> &'static str {
        const TAGS: [&str; 10] = [
            "call", "read", "write", "import", "extends", "macro", "type", "value", "through",
            "access",
        ];
        TAGS[self as usize]
    }
}

impl std::fmt::Display for RefRole {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.tag())
    }
}

pub use crate::languages::Lang;

impl Lang {
    #[inline]
    pub fn tag(self) -> &'static str {
        self.semantics().tag()
    }

    /// Map file extensions to their corresponding language.
    pub fn from_extension(ext: &str) -> Self {
        crate::languages::ALL_LANGS
            .into_iter()
            .find(|l| l.semantics().matches_extension(ext))
            .unwrap_or(Self::Unknown)
    }

    /// Resolve language from a file path (checking extension first, then filename).
    #[must_use]
    pub fn from_path(path: &std::path::Path) -> Self {
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let lang = Self::from_extension(ext);
            if lang != Self::Unknown {
                return lang;
            }
        }
        path.file_name()
            .and_then(|f| f.to_str())
            .map_or(Self::Unknown, Self::from_extension)
    }

    /// True if these two languages are compatible for cross-symbol resolution.
    pub fn is_compatible(self, other: Self) -> bool {
        self == other || self.semantics().dialect_group() == other.semantics().dialect_group()
    }

    /// True if this language has syntax-backed dependency / import capability.
    #[must_use]
    pub fn is_dependency_capable(self) -> bool {
        self.semantics().is_dependency_capable()
    }

    /// True if both languages are dependency capable and share dialect compatibility.
    #[must_use]
    pub fn is_pair_dependency_capable(self, other: Self) -> bool {
        self.is_dependency_capable() && other.is_dependency_capable() && self.is_compatible(other)
    }
}

impl std::fmt::Display for Lang {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.tag())
    }
}

/// What a `Var` binding permits. Only `Mutable` bindings are state on their own; a `Const` binding is
/// state only when something in the indexed files mutates its contents; a `Frozen` binding is never state.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Binding {
    /// Reassignable (`let`, `var`, or any binding the language cannot prove constant).
    #[default]
    Mutable,
    /// Not reassignable, but its value may be a mutable object or array.
    Const,
    /// Not reassignable and holds an immutable value (a literal, a pure expression over scalars).
    Frozen,
}

/// Indexed definition in the symbol table.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    pub id: NodeId,
    pub name: Spur,
    pub canonical_id: Spur,
    pub scope: Option<Spur>,
    pub kind: SymKind,
    pub lang: Lang,
    pub file_id: u32,
    pub line: u32,
    pub sig_start_byte: u32,
    pub sig_end_byte: u32,
    /// Bytes of the doc comment attached above `sig_start_byte`, with the gap to it; 0 when none.
    pub doc_len: u32,
    pub end_byte: u32,
    pub cx: u32,
    pub ccx: u32,
    pub loc: u32,
    pub max_nest: u16,
    pub params: u16,
    pub humps: u16,
    pub deep: u16,
    pub ev: u16,
    /// How the binding may change: a `Frozen` or unmutated `Const` read is not mutable state.
    pub binding: Binding,
    /// A bodyless interface member typed as a function (`f: () => T`): reached by typed dispatch and
    /// conformance, never by a by-name call (its name is not evidence of the callee).
    pub contract_member: bool,
    pub takes_self: bool,
    /// A getter or property: reading it as a member runs it.
    #[serde(default)]
    pub accessor: bool,
}

impl Symbol {
    pub fn new(
        id: NodeId,
        name: Spur,
        canonical_id: Spur,
        kind: SymKind,
        lang: Lang,
        file_id: u32,
        line: u32,
    ) -> Self {
        Self {
            id,
            name,
            canonical_id,
            scope: None,
            kind,
            lang,
            file_id,
            line,
            sig_start_byte: 0,
            doc_len: 0,
            sig_end_byte: 0,
            end_byte: 0,
            cx: 1,
            ccx: 0,
            loc: 1,
            max_nest: 0,
            params: 0,
            humps: 0,
            deep: 0,
            ev: 1,
            binding: Binding::Mutable,
            contract_member: false,
            takes_self: false,
            accessor: false,
        }
    }

    #[must_use]
    pub fn with_scope(mut self, scope: Spur) -> Self {
        self.scope = Some(scope);
        self
    }

    #[inline]
    #[must_use]
    pub const fn is_callable(&self) -> bool {
        self.kind.is_callable()
    }

    /// True when 1-based `line_num` falls within this definition's lines.
    #[inline]
    #[must_use]
    pub const fn spans_line(&self, line_num: u32) -> bool {
        self.line <= line_num && line_num < self.line + if self.loc == 0 { 1 } else { self.loc }
    }

    #[inline]
    #[must_use]
    pub const fn is_code_symbol(&self) -> bool {
        self.kind.is_code_symbol()
    }

    #[inline]
    #[must_use]
    pub const fn has_body(&self) -> bool {
        self.sig_end_byte < self.end_byte
    }

    /// A declaration with no body (a trait method signature): its signature runs to the symbol's
    /// end. A zero span is unknown, not a declaration.
    #[inline]
    #[must_use]
    pub const fn is_bodyless_declaration(&self) -> bool {
        self.end_byte > 0 && self.sig_end_byte >= self.end_byte
    }

    #[inline]
    #[must_use]
    pub const fn end_line(&self) -> u32 {
        self.line.saturating_add(self.loc.saturating_sub(1))
    }

    /// Byte where the definition's span starts: its attached doc comment, else its declaration.
    #[inline]
    #[must_use]
    pub const fn def_start_byte(&self) -> u32 {
        self.sig_start_byte.saturating_sub(self.doc_len)
    }

    /// Return the raw byte slice `[sig_start_byte..end_byte]` (the declaration, without its doc comment).
    #[inline]
    #[must_use]
    pub fn decl_bytes<'a>(&self, file_bytes: &'a [u8]) -> &'a [u8] {
        let start = (self.sig_start_byte as usize).min(file_bytes.len());
        let end = (self.end_byte as usize).min(file_bytes.len());
        if start < end {
            &file_bytes[start..end]
        } else {
            &[]
        }
    }

    /// Return the raw byte slice `[def_start_byte..end_byte]`: the definition as shown to a reader.
    #[inline]
    #[must_use]
    pub fn source_bytes<'a>(&self, file_bytes: &'a [u8]) -> &'a [u8] {
        let start = (self.def_start_byte() as usize).min(file_bytes.len());
        let end = (self.end_byte as usize).min(file_bytes.len());
        if start < end {
            &file_bytes[start..end]
        } else {
            &[]
        }
    }

    /// Return the UTF-8 source text `[def_start_byte..end_byte]` if non-empty.
    #[inline]
    #[must_use]
    pub fn source_text<'a>(&self, file_bytes: &'a [u8]) -> Option<std::borrow::Cow<'a, str>> {
        let slice = self.source_bytes(file_bytes);
        (!slice.is_empty()).then(|| String::from_utf8_lossy(slice))
    }

    /// Extract a single-line normalized declaration signature bounded before any body block.
    #[must_use]
    pub fn extract_signature(&self, file_bytes: &[u8], fallback_name: &str) -> String {
        let start = (self.sig_start_byte as usize).min(file_bytes.len());
        let end = (self.sig_end_byte as usize).min(file_bytes.len());
        if start >= end {
            return fallback_name.to_string();
        }
        let raw = String::from_utf8_lossy(&file_bytes[start..end]);
        // A signature ending before its end_byte is bounded by the parser (the body starts there), so
        // braces inside the header are type syntax (`(): { a: string }`), not the body.
        let header = if self.sig_end_byte < self.end_byte {
            raw.trim()
        } else {
            truncate_at_declaration_body(&raw)
        };
        let collapsed = header.split_whitespace().collect::<Vec<_>>().join(" ");
        if collapsed.is_empty() {
            fallback_name.to_string()
        } else if collapsed.len() > 320 {
            let cut = collapsed.floor_char_boundary(320);
            format!("{}...", collapsed[..cut].trim_end())
        } else {
            collapsed
        }
    }
}

fn truncate_at_declaration_body(raw: &str) -> &str {
    let mut paren_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut angle_depth = 0i32;
    let mut prev = 0u8;
    let mut first_stop = None;
    for (idx, b) in raw.bytes().enumerate() {
        let top = paren_depth == 0 && bracket_depth == 0;
        match b {
            b'(' => paren_depth += 1,
            b')' => paren_depth = (paren_depth - 1).max(0),
            b'[' => bracket_depth += 1,
            b']' => bracket_depth = (bracket_depth - 1).max(0),
            b'<' if top => angle_depth += 1,
            b'>' if top && prev != b'=' => angle_depth = (angle_depth - 1).max(0),
            b'{' | b';' if top => {
                if angle_depth == 0 {
                    return raw[..idx].trim();
                }
                first_stop.get_or_insert(idx);
            }
            _ => {}
        }
        prev = b;
    }
    match first_stop.filter(|_| angle_depth != 0) {
        Some(idx) => raw[..idx].trim(),
        None => raw.trim().trim_end_matches('{').trim(),
    }
}

/// Unresolved reference extracted from AST syntax.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    pub from_symbol: NodeId,
    pub callee_name: Spur,
    pub qualifier: Option<Spur>,
    pub receiver_type: Option<Spur>,
    pub file_id: u32,
    pub line: u32,
    pub name_line: u32,
    pub start_byte: u32,
    pub lang: Lang,
    pub role: RefRole,
    pub arg_count: u16,
    pub arg_count_known: bool,
    pub callable_field: bool,
    /// The call is written inside a macro argument list, where the name cannot be confirmed as a call.
    pub macro_arg: bool,
    /// The reference is a member access (`x.f()`), so its target is a property, not a bare name.
    pub member_call: bool,
}

impl Reference {
    pub fn new(
        from_symbol: NodeId,
        callee_name: Spur,
        file_id: u32,
        line: u32,
        lang: Lang,
        role: RefRole,
    ) -> Self {
        Self {
            from_symbol,
            callee_name,
            qualifier: None,
            receiver_type: None,
            file_id,
            line,
            name_line: line,
            start_byte: 0,
            lang,
            role,
            arg_count: 0,
            arg_count_known: false,
            callable_field: false,
            macro_arg: false,
            member_call: false,
        }
    }

    #[must_use]
    pub fn with_qualifier(mut self, qualifier: Spur) -> Self {
        self.qualifier = Some(qualifier);
        self
    }
}

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
