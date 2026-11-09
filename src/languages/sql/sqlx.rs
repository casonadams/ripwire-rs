//! Dataform `.sqlx` models: a `config { ... }` block, then SQL with `${ref("model")}` substitutions.
//!
//! The file defines one table symbol (the config `name`, else the file stem). Each `ref(...)` or
//! `resolve(...)` call, each `${expr}.schema.table` path, each literal `FROM`/`JOIN` path and each
//! `dependencies` entry is a read of the table it names. The SQL grammar is not involved: the config block
//! is JavaScript and the substitutions are not SQL, so extraction is a scan over comment-masked text.
use super::dataform_vars;
use crate::domain::{RefRole, SymKind};
use crate::ingest::{FileParseResult, RawDef, RawRef};
use std::ops::Range;

#[must_use]
pub fn parse_sqlx(source: &[u8], rel_path: &str) -> FileParseResult {
    let line_count = memchr::memchr_iter(b'\n', source).count() as u32 + 1;
    let mut res = FileParseResult {
        line_count,
        byte_size: source.len() as u64,
        ..FileParseResult::default()
    };
    let Ok(raw) = std::str::from_utf8(source) else {
        return res;
    };
    let masked = mask_comments(raw);
    let text = masked.as_str();
    let config = config_block(text);
    let name = config
        .clone()
        .and_then(|block| top_level_string(&text[block], "name"))
        .unwrap_or_else(|| file_stem(rel_path));
    res.defs.push(RawDef {
        loc: line_count,
        ..RawDef::span(name.to_string(), SymKind::Class, 1, 0..source.len())
    });
    let mut targets = ref_call_targets(text);
    targets.extend(qualified_table_targets(text));
    targets.extend(literal_table_targets(
        text,
        config.as_ref().map_or(0, |block| block.end),
    ));
    if let Some(block) = config {
        targets.extend(dependency_targets(text, block));
    }
    res.refs = targets
        .into_iter()
        .map(|(byte, target)| read_ref(text, byte, target))
        .collect();
    res
}

fn read_ref(text: &str, byte: usize, target: &str) -> RawRef {
    let line = memchr::memchr_iter(b'\n', &text.as_bytes()[..byte]).count() as u32 + 1;
    RawRef {
        callee_name: target.to_string(),
        qualifier: None,
        line,
        name_line: line,
        start_byte: byte,
        role: RefRole::Read,
        arg_count: 0,
        arg_count_known: false,
        lang: None,
        receiver: None,
        receiver_type: None,
        receiver_chain: None,
        receiver_file: None,
        callable_field: false,
        macro_arg: false,
        member_call: false,
        path_call: false,
    }
}

pub(super) fn file_stem(rel_path: &str) -> &str {
    let file = rel_path.rsplit('/').next().unwrap_or(rel_path);
    file.split_once('.').map_or(file, |(stem, _)| stem)
}

/// `text` with `--`, `//` and `/* */` comments blanked to spaces, so offsets and line numbers are unchanged.
fn mask_comments(text: &str) -> String {
    let src = text.as_bytes();
    let mut out = src.to_vec();
    let mut i = 0;
    while i < src.len() {
        i = match (src[i], src.get(i + 1)) {
            (b'"' | b'\'' | b'`', _) => string_end(src, i),
            (b'-', Some(b'-')) | (b'/', Some(b'/')) => blank(&mut out, i, line_end(src, i)),
            (b'/', Some(b'*')) => blank(&mut out, i, block_end(src, i)),
            _ => i + 1,
        };
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn string_end(src: &[u8], at: usize) -> usize {
    let quote = src[at];
    let mut i = at + 1;
    while i < src.len() {
        match src[i] {
            b'\\' => i += 2,
            b if b == quote => return i + 1,
            _ => i += 1,
        }
    }
    src.len()
}

fn line_end(src: &[u8], from: usize) -> usize {
    memchr::memchr(b'\n', &src[from..]).map_or(src.len(), |p| from + p)
}

fn block_end(src: &[u8], from: usize) -> usize {
    memchr::memmem::find(&src[from + 2..], b"*/").map_or(src.len(), |p| from + p + 4)
}

fn blank(out: &mut [u8], from: usize, to: usize) -> usize {
    out[from..to]
        .iter_mut()
        .filter(|b| **b != b'\n')
        .for_each(|b| *b = b' ');
    to
}

/// The text between the braces of a leading `config { ... }` block.
fn config_block(text: &str) -> Option<Range<usize>> {
    let start = text.len() - text.trim_start().len();
    let rest = text[start..].strip_prefix("config")?;
    let open = start + "config".len() + (rest.len() - rest.trim_start().len());
    if text.as_bytes().get(open) != Some(&b'{') {
        return None;
    }
    let mut depth = 0usize;
    let mut i = open;
    while i < text.len() {
        match text.as_bytes()[i] {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(open + 1..i);
                }
            }
            b'"' | b'\'' | b'`' => i = quoted(text, i)?.1 - 1,
            _ => {}
        }
        i += 1;
    }
    None
}

/// The contents and end offset (one past the closing quote) of the string literal opening at `at`.
fn quoted(text: &str, at: usize) -> Option<(&str, usize)> {
    let quote = *text.as_bytes().get(at)?;
    let len = memchr::memchr(quote, &text.as_bytes()[at + 1..])?;
    Some((&text[at + 1..at + 1 + len], at + len + 2))
}

fn skip_space(text: &str, at: usize) -> usize {
    at + (text[at..].len() - text[at..].trim_start().len())
}

fn is_ident_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_' || b == b'$'
}

/// Offset of the value of `key` among the keys of the object body `text`, ignoring nested objects and lists.
fn top_level_value(text: &str, key: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut depth = 0usize;
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'"' | b'\'' | b'`' => i = quoted(text, i)?.1 - 1,
            b'{' | b'[' | b'(' => depth += 1,
            b'}' | b']' | b')' => depth = depth.saturating_sub(1),
            _ if depth == 0 => {
                if let Some(value) = value_after_key(text, i, key) {
                    return Some(value);
                }
            }
            _ => {}
        }
        i += 1;
    }
    None
}

fn value_after_key(text: &str, at: usize, key: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    if !bytes[at..].starts_with(key.as_bytes()) || (at > 0 && is_ident_byte(bytes[at - 1])) {
        return None;
    }
    let colon = skip_space(text, at + key.len());
    (bytes.get(colon) == Some(&b':')).then(|| skip_space(text, colon + 1))
}

fn top_level_string<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let value = top_level_value(text, key)?;
    match text.as_bytes().get(value)? {
        b'"' | b'\'' | b'`' => quoted(text, value).map(|(s, _)| s).filter(|s| !s.is_empty()),
        _ => dataform_vars::expression(&text[value..]),
    }
}

/// `(offset, model)` for each string-literal entry of the config's `dependencies: [...]` list.
fn dependency_targets(text: &str, block: Range<usize>) -> Vec<(usize, &str)> {
    let config = &text[..block.end];
    let Some(list) = top_level_value(&text[block.clone()], "dependencies").map(|rel| block.start + rel) else {
        return Vec::new();
    };
    if config.as_bytes().get(list) != Some(&b'[') {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut at = list + 1;
    while let Some(&b) = config.as_bytes().get(at) {
        match b {
            b']' if depth == 0 => break,
            b'(' | b'[' | b'{' => depth += 1,
            b')' | b']' | b'}' => depth = depth.saturating_sub(1),
            b'"' | b'\'' | b'`' => {
                let Some((name, end)) = quoted(config, at) else {
                    break;
                };
                if depth == 0 {
                    out.push((at + 1, name));
                }
                at = end;
                continue;
            }
            _ => {}
        }
        at += 1;
    }
    out
}

/// `(offset, model)` for each `ref("model")`, `ref("schema", "model")`, `ref({ name: "model" })` and
/// `resolve("model")` call whose arguments are all string literals.
fn ref_call_targets(text: &str) -> Vec<(usize, &str)> {
    ["ref(", "resolve("]
        .into_iter()
        .flat_map(|call| {
            memchr::memmem::find_iter(text.as_bytes(), call.as_bytes())
                .filter(|&at| !at.checked_sub(1).is_some_and(|i| is_ident_byte(text.as_bytes()[i])))
                .filter_map(move |at| ref_argument(text, at + call.len()))
        })
        .collect()
}

/// `(offset, table)` for each `${expr}.schema.table` read: a substitution that is not a `ref`, `resolve` or
/// `self` call followed by a dotted path, whose last segment names the table.
fn qualified_table_targets(text: &str) -> Vec<(usize, &str)> {
    let bytes = text.as_bytes();
    memchr::memmem::find_iter(bytes, b"${")
        .filter_map(|at| {
            let open = at + "${".len();
            let close = open + memchr::memchr(b'}', &bytes[open..])?;
            let inner = text[open..close].trim_start();
            if ["ref(", "resolve(", "self("].iter().any(|call| inner.starts_with(call)) {
                return None;
            }
            let mut end = close + 1;
            let mut last = None;
            while bytes.get(end) == Some(&b'.') {
                let start = end + 1 + usize::from(bytes.get(end + 1) == Some(&b'`'));
                let len = bytes[start..]
                    .iter()
                    .take_while(|b| b.is_ascii_alphanumeric() || **b == b'_')
                    .count();
                if len == 0 {
                    break;
                }
                last = Some((start, &text[start..start + len]));
                end = start + len + usize::from(bytes.get(start + len) == Some(&b'`'));
            }
            last
        })
        .collect()
}

/// `(offset, table)` for each `FROM`/`JOIN` followed by a literal dotted path (`schema.table`,
/// `` `project.schema.table` ``), searched from `from`. An undotted name is a CTE or alias, not a table.
fn literal_table_targets(text: &str, from: usize) -> Vec<(usize, &str)> {
    let bytes = text.as_bytes();
    (from..bytes.len())
        .filter_map(|at| literal_table(text, keyword_end(bytes, at)?))
        .collect()
}

fn keyword_end(bytes: &[u8], at: usize) -> Option<usize> {
    if at > 0 && is_ident_byte(bytes[at - 1]) {
        return None;
    }
    let word = ["from", "join"]
        .into_iter()
        .find(|word| bytes[at..].len() >= word.len() && bytes[at..at + word.len()].eq_ignore_ascii_case(word.as_bytes()))?;
    let end = at + word.len();
    bytes.get(end).filter(|b| b.is_ascii_whitespace()).map(|_| end)
}

fn literal_table(text: &str, after: usize) -> Option<(usize, &str)> {
    let bytes = text.as_bytes();
    let mut start = skip_space(text, after);
    start += usize::from(bytes.get(start) == Some(&b'`'));
    let len = bytes[start..]
        .iter()
        .take_while(|b| b.is_ascii_alphanumeric() || matches!(**b, b'_' | b'.' | b'-'))
        .count();
    let path = text[start..start + len].trim_end_matches('.');
    let (_, table) = path.rsplit_once('.')?;
    (!table.is_empty()).then(|| (start + path.len() - table.len(), table))
}

/// Index of the `)` closing a call whose arguments start at `from`, skipping string literals.
fn call_close(text: &str, from: usize) -> Option<usize> {
    let mut i = from;
    while let Some(&b) = text.as_bytes().get(i) {
        match b {
            b')' => return Some(i),
            b'"' | b'\'' | b'`' => i = quoted(text, i)?.1 - 1,
            _ => {}
        }
        i += 1;
    }
    None
}

fn ref_argument(text: &str, args: usize) -> Option<(usize, &str)> {
    let call = &text[args..call_close(text, args)?];
    if let Some(object) = call.trim_start().strip_prefix('{') {
        return top_level_string(object, "name").map(|name| (args, name));
    }
    let arg = call.trim();
    if let Some(expr) = dataform_vars::expression(arg).filter(|e| e.len() == arg.len()) {
        return Some((args + call.len() - call.trim_start().len(), expr));
    }
    let mut last = None;
    let mut at = 0;
    while let Some(&b) = call.as_bytes().get(at) {
        match b {
            b'"' | b'\'' | b'`' => {
                let (name, end) = quoted(call, at)?;
                last = Some((args + at + 1, name));
                at = end;
            }
            b',' | b' ' | b'\t' | b'\r' | b'\n' => at += 1,
            _ => return None,
        }
    }
    last.filter(|(_, name)| !name.is_empty() && !name.contains("${"))
}
