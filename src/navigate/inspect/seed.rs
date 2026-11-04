use crate::domain::{IngestResult, Interner};
use std::io::{self, Write};

pub fn resolve_symbol_chain_at<'a>(
    file_spec: &str,
    line_num: u32,
    ingest: &'a IngestResult,
    interner: &Interner,
) -> Vec<&'a crate::domain::Symbol> {
    let mut chain: Vec<&'a crate::domain::Symbol> = ingest
        .symbols
        .iter()
        .filter(|sym| {
            let p = interner.resolve(&ingest.files[sym.file_id as usize].path);
            sym.kind != crate::domain::SymKind::ModuleScope
                && (p == file_spec || p.ends_with(file_spec))
                && sym.spans_line(line_num)
        })
        .collect();
    chain.sort_by_key(|s| (s.line, std::cmp::Reverse(s.loc)));
    chain
}

pub fn resolve_symbol_at<'a>(
    file_spec: &str,
    line_num: u32,
    ingest: &'a IngestResult,
    interner: &Interner,
) -> Option<&'a crate::domain::Symbol> {
    resolve_symbol_chain_at(file_spec, line_num, ingest, interner)
        .into_iter()
        .last()
}

/// Whether `selector` is shaped like a `@FILE:LINE` seed. A name that merely starts with `@` (a decorator or an
/// instance variable) is not one, and stays an ordinary name selector.
#[must_use]
pub fn is_seed(selector: &str) -> bool {
    selector
        .strip_prefix('@')
        .and_then(|body| body.rsplit_once(':'))
        .is_some_and(|(file, line)| {
            !file.is_empty() && !line.is_empty() && line.bytes().all(|b| b.is_ascii_digit())
        })
}

/// `selector` as the bare definition name a name-matching scan needs: a `@FILE:LINE` seed becomes the name of the
/// definition it resolves to, anything else (or an unresolvable seed) is returned unchanged.
#[must_use]
pub fn scan_name(selector: &str, ingest: &IngestResult, interner: &Interner) -> String {
    Some(selector)
        .filter(|s| is_seed(s))
        .and_then(|s| resolve_seed(s, ingest, interner).ok())
        .map_or_else(|| selector.to_string(), |seed| seed.name)
}

/// A `@FILE:LINE` selector resolved to the innermost definition enclosing that line.
pub struct ResolvedSeed {
    pub id: crate::domain::NodeId,
    pub name: String,
}

/// Resolve a `@FILE:LINE` seed (1-based line). Every failure names its cause; nothing is guessed.
pub fn resolve_seed(
    spec: &str,
    ingest: &IngestResult,
    interner: &Interner,
) -> Result<ResolvedSeed, String> {
    let (file, line) = spec
        .strip_prefix('@')
        .and_then(|body| body.rsplit_once(':'))
        .and_then(|(file, line)| Some((file, line.parse::<u32>().ok().filter(|n| *n > 0)?)))
        .filter(|(file, _)| !file.is_empty())
        .ok_or_else(|| {
            format!("seed '{spec}' is malformed: expected @FILE:LINE with a 1-based line")
        })?;
    let paths: Vec<&str> = ingest
        .files
        .iter()
        .map(|f| interner.resolve(&f.path))
        .filter(|p| *p == file || p.ends_with(file))
        .collect();
    let path = match paths.as_slice() {
        [] => return Err(format!("seed '{spec}' named no indexed file")),
        [only] => *only,
        many if many.contains(&file) => file,
        many => return Err(format!("seed '{spec}' is ambiguous: {}", many.join(", "))),
    };
    let chain: Vec<_> = resolve_symbol_chain_at(path, line, ingest, interner)
        .into_iter()
        .filter(|s| interner.resolve(&ingest.files[s.file_id as usize].path) == path)
        .collect();
    let Some(&innermost) = chain.last() else {
        return Err(format!(
            "seed '{spec}': no indexed definition spans line {line}"
        ));
    };
    if let [.., prev, last] = chain.as_slice()
        && prev.line == last.line
        && prev.loc == last.loc
    {
        return Err(format!("seed '{spec}': two definitions share line {line}"));
    }
    Ok(ResolvedSeed {
        id: innermost.id,
        name: interner.resolve(&innermost.name).to_string(),
    })
}

pub fn serialize_at_xml<W: Write>(
    file_spec: &str,
    line_num: u32,
    ingest: &IngestResult,
    interner: &Interner,
    out: &mut W,
) -> io::Result<()> {
    let exact_match = ingest
        .files
        .iter()
        .any(|f| interner.resolve(&f.path) == file_spec);
    let suffix_matches: Vec<_> = ingest
        .files
        .iter()
        .filter(|f| {
            let p = interner.resolve(&f.path);
            p == file_spec || p.ends_with(file_spec)
        })
        .collect();

    if suffix_matches.is_empty() {
        return Err(io::Error::other(format!(
            "ripwire-rs: the at flag's seed '{file_spec}:{line_num}' named no location"
        )));
    }
    if suffix_matches.len() > 1 && !exact_match {
        return Err(io::Error::other(format!(
            "ripwire-rs: the at flag's seed '{file_spec}:{line_num}' named no location (ambiguous)"
        )));
    }
    let chain = resolve_symbol_chain_at(file_spec, line_num, ingest, interner);
    let Some(&innermost) = chain.last() else {
        return Err(io::Error::other(format!(
            "ripwire-rs: the at flag's seed '{file_spec}:{line_num}' named no location (no indexed symbol spans line {line_num})"
        )));
    };
    let name = interner.resolve(&innermost.name);
    let chain_len = chain.len();
    write!(
        out,
        "<!-- ripwire-rs at schema=ripwire-rs.at/v1: enclosing-definition chain at p=:l=. -->\n<at schema=\"ripwire-rs.at/v1\" p=\"{file_spec}\" l=\"{line_num}\" sym=\"{name}\" chain=\"{chain_len}\" root=\".\">",
    )?;
    for sym in chain {
        let sn = interner.resolve(&sym.name);
        let kind = sym.kind.tag();
        let end_l = sym.line.saturating_add(sym.loc.saturating_sub(1));
        write!(
            out,
            "<s n=\"{sn}\" t=\"{kind}\" l=\"{}\" el=\"{end_l}\"/>",
            sym.line
        )?;
    }
    writeln!(out, "</at>")
}
