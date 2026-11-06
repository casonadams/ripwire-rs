use crate::domain::{RefRole, SymKind};

/// Raw definition extracted before deduplication and global ID assignment.
#[derive(Debug, Clone, Default)]
pub struct RawDef {
    pub name: String,
    pub scope: Option<String>,
    pub kind: SymKind,
    pub line: u32,
    pub start_byte: usize,
    /// Bytes from the attached doc comment (if any) up to `start_byte`; 0 when none.
    pub doc_len: usize,
    pub body_byte: usize,
    pub end_byte: usize,
    pub loc: u32,
    pub params: u16,
    /// Parameters a call must bind: `params` less those with a default value.
    pub required_params: u16,
    pub metrics: crate::metrics::ComplexityMetrics,
    /// Set by the language plugin for `Var` bindings (see `var_binding`); other kinds are `Mutable`.
    pub binding: crate::domain::Binding,
    /// Set by the language plugin for bodyless members reached only by typed dispatch (see `is_contract_member`).
    pub contract_member: bool,
    pub takes_self: bool,
    pub accessor: bool,
    pub registered: bool,
    pub constructor: bool,
}

impl RawDef {
    #[must_use]
    pub fn span(name: String, kind: SymKind, line: u32, bytes: std::ops::Range<usize>) -> Self {
        Self {
            name,
            kind,
            line,
            start_byte: bytes.start,
            body_byte: bytes.start,
            end_byte: bytes.end,
            loc: 1,
            ..Self::default()
        }
    }
}

/// A receiver chain whose base type is bound in its own file, while its member steps may be declared in
/// another: `doc.select()` has base `Document` and step `select()`. Typed once every file is parsed.
#[derive(Debug, Clone)]
pub struct PendingChain {
    pub base: String,
    pub steps: Vec<String>,
}

/// A member a class declares: `owner.member` (a method is `name()`) has declared type `ty`. The member
/// `EXTENDS` names a class's superclass instead.
#[derive(Debug, Clone)]
pub struct MemberType {
    pub owner: String,
    pub member: String,
    pub ty: String,
}

/// Raw reference extracted before enclosing symbol attribution.
#[derive(Debug, Clone)]
pub struct RawRef {
    pub callee_name: String,
    pub qualifier: Option<String>,
    pub line: u32,
    pub name_line: u32,
    pub start_byte: usize,
    pub role: RefRole,
    pub arg_count: u16,
    pub arg_count_known: bool,
    /// Language the reference is written in when it differs from its file's (embedded SQL).
    pub lang: Option<crate::domain::Lang>,
    /// Bare identifier receiver (`x` in `x.f()`) not kept as the qualifier.
    pub receiver: Option<String>,
    pub receiver_type: Option<String>,
    pub callable_field: bool,
    pub macro_arg: bool,
    /// The reference is written as a member access (`x.f()`), not a bare or path name.
    pub member_call: bool,
    /// The name is reached through a path (`Type::f`), so a method's receiver is written as its first argument.
    pub path_call: bool,
    /// The receiver chain this reference could not type within its file, for the cross-file pass.
    pub receiver_chain: Option<PendingChain>,
    /// The file that declares the member a cross-file chain typed `receiver_type` through. The type name is
    /// written in that file, so its imports (not the reference's own file's) name the type.
    pub receiver_file: Option<u32>,
}

/// Parsed facts from a single file.
#[derive(Debug, Default)]
pub struct FileParseResult {
    pub line_count: u32,
    pub byte_size: u64,
    pub defs: Vec<RawDef>,
    pub refs: Vec<RawRef>,
    pub foreign_exports: Vec<crate::languages::ForeignExport>,
    pub callable_bindings: Vec<crate::languages::CallableBinding>,
    pub members: Vec<MemberType>,
    pub implementations: Vec<crate::languages::Implementation>,
    /// References to a field the file does not declare, reached through `this` of the class `class`.
    pub pending_fields: Vec<PendingField>,
    /// Type aliases this file declares, as `(alias, declared target type)`; set only where the language types
    /// fields across files.
    pub aliases: Vec<(String, String)>,
}

/// A receiver `member` (or `this.member`) of `refs[ref_index]`, reached through `this` of class `class`, whose field
/// no binding in the reference's own file declares.
#[derive(Debug, Clone)]
pub struct PendingField {
    pub ref_index: usize,
    pub class: String,
    pub member: String,
}
