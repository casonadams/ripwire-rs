use crate::domain::{IngestResult, Interner, SymKind};
use crate::serialize::{PageRequest, XmlEscapeExt, row_bytes};
use std::io::{self, Write};

pub fn classify_identifier_casing(name: &str) -> &'static str {
    let clean = name.trim_start_matches('_');
    if clean.is_empty() {
        return "other";
    }
    let has_underscore = clean.contains('_');
    let has_upper = clean.chars().any(char::is_uppercase);
    let has_lower = clean.chars().any(char::is_lowercase);
    let first_upper = clean.chars().next().is_some_and(char::is_uppercase);

    if has_underscore && !has_lower && has_upper {
        "SCREAMING_SNAKE_CASE"
    } else if has_underscore && !has_upper {
        "snake_case"
    } else if !has_underscore && first_upper && has_lower {
        "PascalCase"
    } else if !has_underscore && !first_upper && has_upper && has_lower {
        "camelCase"
    } else if !has_underscore && !has_upper && has_lower {
        "lower"
    } else {
        "mixed"
    }
}

fn write_vote_row<W: Write + ?Sized>(
    w: &mut W,
    kind: &str,
    style: &str,
    count: usize,
) -> io::Result<()> {
    writeln!(
        w,
        "  <vote kind=\"{kind}\" style=\"{style}\" count=\"{count}\"/>"
    )
}

pub fn serialize_naming_consistency_xml<W: Write>(
    root: &str,
    ingest: &IngestResult,
    interner: &Interner,
    page: PageRequest,
    out: &mut W,
) -> io::Result<()> {
    let mut counts: std::collections::BTreeMap<(&'static str, &'static str), usize> =
        std::collections::BTreeMap::new();
    for sym in &ingest.symbols {
        if sym.kind == SymKind::ModuleScope {
            continue;
        }
        let name = interner.resolve(&sym.name);
        let style = classify_identifier_casing(name);
        *counts.entry((sym.kind.tag(), style)).or_insert(0) += 1;
    }
    writeln!(
        out,
        "<naming-consistency schema=\"ripwire-rs.naming-consistency/v1\" root=\"{}\" symbols=\"{}\">",
        root.xml_escaped(),
        ingest.symbol_count()
    )?;
    let votes: Vec<_> = counts.into_iter().collect();
    let pw = page.window_fit(votes.len(), |i| {
        let ((kind, style), count) = &votes[i];
        row_bytes(|w| write_vote_row(w, kind, style, *count))
    });
    for ((kind, style), count) in &votes[pw.begin..pw.end] {
        write_vote_row(out, kind, style, *count)?;
    }
    writeln!(out, "</naming-consistency>")
}
