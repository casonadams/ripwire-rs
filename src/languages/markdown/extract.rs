use crate::ingest::{FileParseResult, RawDef, RawRef};
use crate::domain::{RefRole, SymKind};
use std::cell::RefCell;
use tree_sitter::{Language, Node, Parser};

thread_local! {
    static MD_PARSER: RefCell<Parser> = RefCell::new(Parser::new());
}

/// Clean heading text by stripping carriage returns and trailing hashes/spaces.
fn clean_heading_text(raw: &str) -> String {
    let mut name = String::with_capacity(raw.len());
    for c in raw.chars() {
        if c != '\r' {
            name.push(if c == '\n' { ' ' } else { c });
        }
    }
    let trimmed = name.trim();
    trimmed.trim_matches('#').trim().to_string()
}

fn determine_atx_heading(node: Node<'_>, source_bytes: &[u8]) -> Option<(u32, String)> {
    let mut level = 1u32;
    let mut text = String::new();
    for i in 0..node.child_count() {
        let Some(child) = node.child(i) else { continue };
        let ckind = child.kind();
        if ckind.starts_with("atx_h") && ckind.ends_with("_marker") {
            if let Ok(marker) = child.utf8_text(source_bytes) {
                level = marker.chars().filter(|&c| c == '#').count() as u32;
            }
        } else if ckind == "heading_content"
            && let Ok(content) = child.utf8_text(source_bytes)
        {
            text = clean_heading_text(content);
        }
    }
    if text.is_empty()
        && let Ok(raw) = node.utf8_text(source_bytes)
    {
        text = clean_heading_text(raw);
    }
    Some((level.clamp(1, 6), text))
}

fn extract_paragraph_inline_text(child: Node<'_>, source_bytes: &[u8]) -> Option<String> {
    for j in 0..child.named_child_count() {
        let inline = child.named_child(j)?;
        if inline.kind() == "inline" {
            let content = inline.utf8_text(source_bytes).ok()?;
            return Some(clean_heading_text(content));
        }
    }
    child.utf8_text(source_bytes).ok().map(clean_heading_text)
}

fn determine_setext_heading(node: Node<'_>, source_bytes: &[u8]) -> Option<(u32, String)> {
    let mut level = 1u32;
    let mut text = String::new();
    for i in 0..node.named_child_count() {
        let Some(child) = node.named_child(i) else {
            continue;
        };
        match child.kind() {
            "setext_h1_underline" => level = 1,
            "setext_h2_underline" => level = 2,
            "paragraph" => {
                if let Some(t) = extract_paragraph_inline_text(child, source_bytes) {
                    text = t;
                }
            }
            _ => {}
        }
    }
    if text.is_empty() {
        None
    } else {
        Some((level, text))
    }
}

fn determine_heading_level(node: Node<'_>, source_bytes: &[u8]) -> Option<(u32, String)> {
    match node.kind() {
        "atx_heading" => determine_atx_heading(node, source_bytes),
        "setext_heading" => determine_setext_heading(node, source_bytes),
        _ => None,
    }
}

struct HeadingEntry {
    level: u32,
    name: String,
    line: u32,
    start_byte: usize,
}

fn walk_markdown_tree<'a>(node: Node<'a>, source_bytes: &[u8], headings: &mut Vec<HeadingEntry>) {
    let kind = node.kind();
    if kind == "fenced_code_block" || kind == "indented_code_block" || kind == "html_block" {
        return;
    }

    if let Some((level, name)) = determine_heading_level(node, source_bytes)
        && !name.is_empty()
    {
        headings.push(HeadingEntry {
            level,
            name,
            line: node.start_position().row as u32 + 1,
            start_byte: node.start_byte(),
        });
    }

    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            walk_markdown_tree(child, source_bytes, headings);
        }
    }
}

/// `[text](target)` links in one line: the byte offset of each opening `[` and its raw target.
fn line_link_targets(line: &str) -> Vec<(usize, &str)> {
    let bytes = line.as_bytes();
    let mut links = Vec::new();
    let mut b = 0;
    while b + 1 < bytes.len() {
        if bytes[b] != b'[' {
            b += 1;
            continue;
        }
        if bytes[b + 1] == b'[' {
            b += 2;
            continue;
        }
        let Some(close_rel) = bytes[b + 1..].iter().position(|&c| c == b']') else {
            break;
        };
        let close = b + 1 + close_rel;
        if close + 1 < bytes.len()
            && bytes[close + 1] == b'('
            && let Some(rp_rel) = bytes[close + 2..].iter().position(|&c| c == b')')
        {
            let rp = close + 2 + rp_rel;
            links.push((b, line[close + 2..rp].trim()));
            b = rp + 1;
            continue;
        }
        b = close + 1;
    }
    links
}

/// Raw Markdown link targets in `text`, each including any `#anchor`.
#[must_use]
pub fn markdown_link_targets(text: &str) -> Vec<&str> {
    text.lines()
        .flat_map(line_link_targets)
        .map(|(_, target)| target)
        .collect()
}

/// GitHub-style anchor slug of a heading's text: lowercase, spaces to `-`, other punctuation dropped.
fn heading_slug(text: &str) -> String {
    text.trim()
        .chars()
        .flat_map(char::to_lowercase)
        .filter_map(|c| match c {
            ' ' => Some('-'),
            c if c.is_alphanumeric() || c == '-' || c == '_' => Some(c),
            _ => None,
        })
        .collect()
}

/// The symbol a link target names, or `None` when it names nothing in the repository: an external URL, a
/// non-document file, or an `#anchor` that matches no heading of this document. A same-document anchor names
/// the heading it slugs to; a link to another Markdown file names that file's root section, by its stem.
fn link_callee(target: &str, headings: &[(String, String)]) -> Option<String> {
    let target = target.trim_matches(['<', '>']);
    let target = target.split_whitespace().next()?;
    if target.contains("://") || target.starts_with("mailto:") || target.starts_with("//") {
        return None;
    }
    if let Some(anchor) = target.strip_prefix('#') {
        return headings
            .iter()
            .find(|(slug, _)| slug.as_str() == anchor)
            .map(|(_, name)| name.clone());
    }
    let path = target.split('#').next()?;
    let file = path.rsplit('/').next()?;
    let (stem, ext) = file.rsplit_once('.')?;
    matches!(ext, "md" | "markdown" | "mdx")
        .then(|| stem.to_string())
        .filter(|stem| !stem.is_empty())
}

fn push_link_ref(callee: String, line: u32, start_byte: usize, refs: &mut Vec<RawRef>) {
    refs.push(RawRef::positioned(callee, RefRole::Call, line, start_byte));
}

/// Byte offset of the backtick run of exactly `run` backticks that closes a code span opened before `from`.
fn closing_backtick_run(bytes: &[u8], from: usize, run: usize) -> Option<usize> {
    let mut p = from;
    while p < bytes.len() {
        if bytes[p] == b'`' {
            let len = bytes[p..].iter().take_while(|&&b| b == b'`').count();
            if len == run {
                return Some(p);
            }
            p += len;
        } else {
            p += 1;
        }
    }
    None
}

/// `text` with the content of every inline code span blanked to spaces, so a link written inside code is not a link.
/// Blanking replaces whole characters between two ASCII backticks, so the result stays valid UTF-8 and offsets hold.
fn mask_code_spans(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut out = bytes.to_vec();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let run = bytes[i..].iter().take_while(|&&b| b == b'`').count();
        match closing_backtick_run(bytes, i + run, run) {
            Some(close) => {
                out[i..close + run].fill(b' ');
                i = close + run;
            }
            None => i += run,
        }
    }
    String::from_utf8(out).unwrap_or_default()
}

/// Links in one prose `inline` node, whose text starts at `start_byte` on line `first_line`.
fn scan_inline_links(
    text: &str,
    first_line: u32,
    start_byte: usize,
    headings: &[(String, String)],
    refs: &mut Vec<RawRef>,
) {
    let masked = mask_code_spans(text);
    let mut line_start = start_byte;
    for (offset, line) in masked.split('\n').enumerate() {
        let line_num = first_line + offset as u32;
        for (b, target) in line_link_targets(line) {
            if let Some(callee) = link_callee(target, headings) {
                push_link_ref(callee, line_num, line_start + b, refs);
            }
        }
        line_start += line.len() + 1;
    }
}

/// Byte ranges of the block tree's prose (`inline`) nodes with their first line. Fenced and indented code, HTML
/// blocks and link reference definitions carry no `inline` node, so links inside them are never scanned.
fn collect_inline_nodes(node: Node<'_>, out: &mut Vec<(usize, usize, u32)>) {
    if node.kind() == "inline" {
        out.push((
            node.start_byte(),
            node.end_byte(),
            node.start_position().row as u32 + 1,
        ));
        return;
    }
    for i in 0..node.child_count() {
        if let Some(child) = node.child(i) {
            collect_inline_nodes(child, out);
        }
    }
}

fn assemble_defs(headings: &[HeadingEntry], rel_path: &str, source_len: usize) -> Vec<RawDef> {
    let mut defs = Vec::with_capacity(headings.len() + 1);
    let stem = std::path::Path::new(rel_path)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("doc");

    defs.push(RawDef::span(
        stem.to_string(),
        SymKind::Section,
        1,
        0..source_len,
    ));

    for (i, h) in headings.iter().enumerate() {
        let mut end_byte = source_len;
        for next in &headings[i + 1..] {
            if next.level <= h.level {
                end_byte = next.start_byte;
                break;
            }
        }

        let mut scope = None;
        for prev in headings[..i].iter().rev() {
            if prev.level < h.level {
                scope = Some(prev.name.clone());
                break;
            }
        }

        defs.push(RawDef {
            scope,
            ..RawDef::span(
                h.name.clone(),
                SymKind::Section,
                h.line,
                h.start_byte..end_byte,
            )
        });
    }

    defs
}

/// Extract symbols and references from Markdown document AST.
#[must_use]
pub fn extract_markdown(
    source_bytes: &[u8],
    rel_path: &str,
    language: &Language,
) -> FileParseResult {
    let tree = MD_PARSER.with(|cell| {
        let parser = &mut *cell.borrow_mut();
        if parser.set_language(language).is_err() {
            return None;
        }
        parser.parse(source_bytes, None)
    });
    let Some(tree) = tree else {
        return FileParseResult::default();
    };

    let mut headings = Vec::new();
    walk_markdown_tree(tree.root_node(), source_bytes, &mut headings);
    let slugs: Vec<(String, String)> = headings
        .iter()
        .map(|h| (heading_slug(&h.name), h.name.clone()))
        .collect();

    let mut prose = Vec::new();
    collect_inline_nodes(tree.root_node(), &mut prose);
    let mut refs = Vec::new();
    if let Ok(src_str) = std::str::from_utf8(source_bytes) {
        for (start, end, row) in prose {
            if let Some(text) = src_str.get(start..end) {
                scan_inline_links(text, row, start, &slugs, &mut refs);
            }
        }
    }
    let defs = assemble_defs(&headings, rel_path, source_bytes.len());
    let line_count = source_bytes.iter().filter(|&&b| b == b'\n').count() as u32 + 1;

    FileParseResult {
        line_count,
        members: Vec::new(),
        byte_size: source_bytes.len() as u64,
        defs,
        refs,
        foreign_exports: Vec::new(),
        callable_bindings: Vec::new(),
        implementations: Vec::new(),
        ..FileParseResult::default()
    }
}
