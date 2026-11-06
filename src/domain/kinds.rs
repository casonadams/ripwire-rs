use crate::languages::Lang;
use serde::{Deserialize, Serialize};

/// Symbol kind corresponding to the terse XML attribute `t="..."`.
#[repr(u8)]
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
    #[default]
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
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RefRole {
    #[default]
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
