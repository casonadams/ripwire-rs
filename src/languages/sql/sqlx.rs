//! Dataform `.sqlx` models: a `config { ... }` block, then SQL with `${ref("model")}` substitutions.
//!
//! The file defines one table symbol (the config `name`, else the file stem). Each `ref(...)` call and each
//! `dependencies` entry is a read of the model it names. The SQL grammar is not involved: the config block is
//! JavaScript and the substitutions are not SQL, so extraction is a scan over the text.
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
    let Ok(text) = std::str::from_utf8(source) else {
        return res;
    };
    let config = config_block(text);
    let name = config
        .clone()
        .and_then(|block| string_for_key(&text[block], "name"))
        .unwrap_or_else(|| file_stem(rel_path));
    res.defs.push(RawDef {
        loc: line_count,
        ..RawDef::span(name.to_string(), SymKind::Class, 1, 0..source.len())
    });
    let mut targets = ref_call_targets(text);
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
        callable_field: false,
        macro_arg: false,
        member_call: false,
        path_call: false,
    }
}

fn file_stem(rel_path: &str) -> &str {
    let file = rel_path.rsplit('/').next().unwrap_or(rel_path);
    file.split_once('.').map_or(file, |(stem, _)| stem)
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

/// Offsets just past each `key` that stands as a whole word followed by `:`.
fn key_values<'a>(text: &'a str, key: &'a str) -> impl Iterator<Item = usize> + 'a {
    memchr::memmem::find_iter(text.as_bytes(), key.as_bytes()).filter_map(move |at| {
        let before = at.checked_sub(1).map(|i| text.as_bytes()[i]);
        if before.is_some_and(is_ident_byte) {
            return None;
        }
        let colon = skip_space(text, at + key.len());
        (text.as_bytes().get(colon) == Some(&b':')).then(|| skip_space(text, colon + 1))
    })
}

fn string_for_key<'a>(text: &'a str, key: &str) -> Option<&'a str> {
    let value = key_values(text, key).next()?;
    quoted(text, value).map(|(s, _)| s).filter(|s| !s.is_empty())
}

/// `(offset, model)` for each entry of the config's `dependencies: [...]` list.
fn dependency_targets(text: &str, block: Range<usize>) -> Vec<(usize, &str)> {
    let config = &text[..block.end];
    let Some(list) = key_values(&config[block.start..], "dependencies")
        .next()
        .map(|rel| block.start + rel)
    else {
        return Vec::new();
    };
    if config.as_bytes().get(list) != Some(&b'[') {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut at = list + 1;
    while let Some(&b) = config.as_bytes().get(at) {
        match b {
            b']' => break,
            b'"' | b'\'' | b'`' => {
                let Some((name, end)) = quoted(config, at) else {
                    break;
                };
                out.push((at + 1, name));
                at = end;
            }
            _ => at += 1,
        }
    }
    out
}

/// `(offset, model)` for each `ref("model")`, `ref("schema", "model")` and `ref({ name: "model" })` call.
fn ref_call_targets(text: &str) -> Vec<(usize, &str)> {
    memchr::memmem::find_iter(text.as_bytes(), b"ref(")
        .filter(|&at| !at.checked_sub(1).is_some_and(|i| is_ident_byte(text.as_bytes()[i])))
        .filter_map(|at| ref_argument(text, at + "ref(".len()))
        .collect()
}

fn ref_argument(text: &str, args: usize) -> Option<(usize, &str)> {
    let close = args + memchr::memchr(b')', &text.as_bytes()[args..])?;
    let call = &text[args..close];
    if call.trim_start().starts_with('{') {
        return string_for_key(call, "name").map(|name| (args, name));
    }
    let mut last = None;
    let mut at = 0;
    while let Some(&b) = call.as_bytes().get(at) {
        if matches!(b, b'"' | b'\'' | b'`') {
            let (name, end) = quoted(call, at)?;
            last = Some((args + at + 1, name));
            at = end;
        } else {
            at += 1;
        }
    }
    last.filter(|(_, name)| !name.is_empty())
}
