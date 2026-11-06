use super::{Binding, Lang, NodeId, SymKind};
use lasso::Spur;
use serde::{Deserialize, Serialize};

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
    /// Parameters a call must bind: `params` less those with a default value.
    #[serde(default)]
    pub required_params: u16,
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
    /// Registered by a decorator (a route, a hook, a task): reached without a call by name.
    #[serde(default)]
    pub registered: bool,
    /// A constructor or factory: reached through its class name (`C.make(...)`), never as an instance member.
    #[serde(default)]
    pub constructor: bool,
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
            required_params: 0,
            humps: 0,
            deep: 0,
            ev: 1,
            binding: Binding::Mutable,
            contract_member: false,
            takes_self: false,
            accessor: false,
            registered: false,
            constructor: false,
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
