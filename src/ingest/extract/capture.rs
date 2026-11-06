use super::def::record_def_item;
use super::macros::scan_macro_body_calls;
use super::refs::record_ref_item;
use super::types::{RawDef, RawRef};
use crate::domain::{Lang, RefRole, SymKind};
use tree_sitter::Query;

pub enum Capture {
    Name,
    ImportPath,
    Def(SymKind, Option<&'static str>),
    Ref(RefRole),
}

pub fn classify_capture(name: &str) -> Option<Capture> {
    if name == "name" {
        return Some(Capture::Name);
    }
    if name == "import.path" {
        return Some(Capture::ImportPath);
    }
    if let Some(tail) = name.strip_prefix("definition.") {
        let (kind, sub) = match tail {
            "function" => (SymKind::Function, None),
            "cjsexport" => (SymKind::Function, Some("cjsexport")),
            "method" => (SymKind::Method, None),
            "protomethod" => (SymKind::Method, Some("protomethod")),
            "class" | "object" => (SymKind::Class, None),
            "specialization" => (SymKind::Class, Some("specialization")),
            "struct" => (SymKind::Struct, None),
            "interface" => (SymKind::Interface, None),
            "constant" => (SymKind::Var, Some("constant")),
            "enummember" => (SymKind::Var, Some("enummember")),
            "attribute" => (SymKind::Var, Some("attribute")),
            "type" | "var" | "property" => (SymKind::Var, None),
            "macro" => (SymKind::Macro, None),
            "testmacroblock" => (SymKind::Macro, Some("testmacroblock")),
            "yamlkey" => (SymKind::Section, Some("yamlkey")),
            "module" | "section" => (SymKind::Section, None),
            "field" => (SymKind::Field, None),
            _ => return None,
        };
        return Some(Capture::Def(kind, sub));
    }
    name.strip_prefix("reference.").and_then(|tail| {
        Some(Capture::Ref(match tail {
            "read" | "attribute" => RefRole::Read,
            "write" => RefRole::Write,
            "import" => RefRole::Import,
            "extends" | "implementation" | "class" => RefRole::Extends,
            "macro" => RefRole::Macro,
            "type" => RefRole::Type,
            "value" => RefRole::Value,
            "through" => RefRole::Through,
            "access" => RefRole::Access,
            "call" | "bare" | "operator" => RefRole::Call,
            _ => return None,
        }))
    })
}

pub(super) struct MatchExtract<'a> {
    pub(super) name: Option<String>,
    pub(super) name_node: Option<tree_sitter::Node<'a>>,
    pub(super) def: Option<(SymKind, Option<&'static str>, usize, usize, u32)>,
    pub(super) def_node: Option<tree_sitter::Node<'a>>,
    pub(super) r#ref: Option<(RefRole, usize, u32)>,
}

fn apply_capture<'a>(
    ext: &mut MatchExtract<'a>,
    cap_name: &str,
    node: tree_sitter::Node<'a>,
    source_bytes: &[u8],
) {
    match classify_capture(cap_name) {
        Some(Capture::Name) => {
            if let Ok(text) = node.utf8_text(source_bytes) {
                ext.name = Some(text.to_string());
                ext.name_node = Some(node);
            }
        }
        Some(Capture::ImportPath) => {
            let line = node.start_position().row as u32 + 1;
            ext.r#ref = Some((RefRole::Import, node.start_byte(), line));
            if let Ok(text) = node.utf8_text(source_bytes) {
                ext.name = Some(text.to_string());
                ext.name_node = Some(node);
            }
        }
        Some(Capture::Def(k, sub)) => {
            let line = node.start_position().row as u32 + 1;
            ext.def = Some((k, sub, node.start_byte(), node.end_byte(), line));
            ext.def_node = Some(node);
        }
        Some(Capture::Ref(r)) => {
            let line = node.start_position().row as u32 + 1;
            ext.r#ref = Some((r, node.start_byte(), line));
        }
        None => {}
    }
}

pub(super) fn extract_match_captures<'a>(
    m: &tree_sitter::QueryMatch<'a, '_>,
    query: &Query,
    source_bytes: &[u8],
) -> MatchExtract<'a> {
    let mut ext = MatchExtract {
        name: None,
        name_node: None,
        def: None,
        def_node: None,
        r#ref: None,
    };
    for capture in m.captures {
        let cap_name = query.capture_names()[capture.index as usize];
        apply_capture(&mut ext, cap_name, capture.node, source_bytes);
    }
    ext
}

pub(super) fn record_extracted_item(
    root: tree_sitter::Node<'_>,
    ext: MatchExtract<'_>,
    source_bytes: &[u8],
    lang: Lang,
    defs: &mut Vec<RawDef>,
    refs: &mut Vec<RawRef>,
) {
    if let Some((kind, _, start, _, line)) = ext.def {
        record_def_item(root, &ext, source_bytes, lang, defs);
        if kind == SymKind::Macro
            && let Some(name) = &ext.name
        {
            scan_macro_body_calls(ext.def_node, name, line, start, lang, source_bytes, refs);
        }
    } else if ext.r#ref.is_some() {
        record_ref_item(root, &ext, source_bytes, lang, refs);
    }
}
