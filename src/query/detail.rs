use crate::domain::{IngestResult, Interner, NodeId, Symbol};
use crate::query::SourceReader;
use crate::serialize::workspace_xml::{BodyItem, BodyReport};
use std::borrow::Cow;
use std::io::{self, Write};

pub struct BodyRequest<'a> {
    pub root: &'a str,
    pub symbols: &'a str,
    pub path_hint: Option<&'a str>,
    pub start_line: Option<usize>,
    pub end_line: Option<usize>,
    pub outline: bool,
    pub compress: bool,
}

pub fn skeletonize_body(body: &str) -> String {
    let mut out = Vec::new();
    let mut depth = 0i32;
    let mut elided = false;
    for line in body.lines() {
        let opens = line.chars().filter(|&c| c == '{').count() as i32;
        let closes = line.chars().filter(|&c| c == '}').count() as i32;
        if depth <= 1 {
            out.push(line);
            elided = false;
        } else if !elided {
            out.push("    ...");
            elided = true;
        }
        depth += opens - closes;
        if depth <= 1 && closes > 0 && elided {
            out.push(line);
            elided = false;
        }
    }
    out.join("\n")
}

struct BodySelection<'a> {
    body: Cow<'a, str>,
    range: Option<(usize, usize, usize)>,
}

fn body_range<'a>(body: &'a str, request: &BodyRequest<'_>) -> io::Result<BodySelection<'a>> {
    if request.start_line.is_none() && request.end_line.is_none() {
        return Ok(BodySelection {
            body: Cow::Borrowed(body),
            range: None,
        });
    }
    let total = body.lines().count();
    let start = request.start_line.unwrap_or(1);
    let requested_end = request.end_line.unwrap_or(total);
    if start == 0 || requested_end < start || start > total {
        return Err(io::Error::other(format!(
            "invalid body line range {start}..{requested_end} for {total} lines"
        )));
    }
    let end = requested_end.min(total);
    let start_byte: usize = body
        .split_inclusive('\n')
        .take(start - 1)
        .map(str::len)
        .sum();
    let end_byte: usize = body.split_inclusive('\n').take(end).map(str::len).sum();
    let selected = &body[start_byte..end_byte];
    let selected = selected.strip_suffix('\n').unwrap_or(selected);
    let selected = selected.strip_suffix('\r').unwrap_or(selected);
    Ok(BodySelection {
        body: Cow::Borrowed(selected),
        range: Some((total, start, end)),
    })
}

struct PreparedBody<'a> {
    symbol: &'a Symbol,
    name: &'a str,
    path: &'a str,
    body: Cow<'a, str>,
    range: Option<(usize, usize, usize)>,
}

fn select_symbols(
    request: &BodyRequest<'_>,
    ingest: &IngestResult,
    interner: &Interner,
) -> io::Result<Vec<NodeId>> {
    let mut selected = Vec::new();
    for name in request
        .symbols
        .split(',')
        .map(str::trim)
        .filter(|name| !name.is_empty())
    {
        let ids = crate::navigate::find_symbols_by_name(name, ingest, interner);
        let previous = selected.len();
        selected.extend(ids.into_iter().filter(|&id| {
            request.path_hint.is_none_or(|hint| {
                ingest
                    .sym_file_path(&ingest.symbols[id as usize], interner)
                    .contains(hint)
            })
        }));
        if selected.len() == previous {
            return Err(io::Error::other(format!("symbol '{name}' not found")));
        }
    }
    if selected.is_empty() {
        return Err(io::Error::other("symbol name is required"));
    }
    Ok(selected)
}

fn read_source_files<'a>(
    selected: &[NodeId],
    ingest: &IngestResult,
    interner: &Interner,
    sources: &'a dyn SourceReader,
) -> io::Result<std::collections::HashMap<u32, Cow<'a, [u8]>>> {
    let mut files = std::collections::HashMap::new();
    for &id in selected {
        let sym = &ingest.symbols[id as usize];
        let path = ingest.sym_file_path(sym, interner);
        if let std::collections::hash_map::Entry::Vacant(entry) = files.entry(sym.file_id) {
            entry.insert(sources.read(path).map_err(|error| {
                io::Error::other(format!("cannot read file '{path}': {error}"))
            })?);
        }
    }
    Ok(files)
}

fn source_texts<'a>(
    selected: &[NodeId],
    ingest: &IngestResult,
    interner: &Interner,
    files: &'a std::collections::HashMap<u32, Cow<'_, [u8]>>,
) -> io::Result<Vec<Cow<'a, str>>> {
    selected
        .iter()
        .map(|&id| {
            let sym = &ingest.symbols[id as usize];
            sym.source_text(&files[&sym.file_id]).ok_or_else(|| {
                io::Error::other(format!(
                    "invalid source range for '{}' in '{}'",
                    interner.resolve(&sym.name),
                    ingest.sym_file_path(sym, interner)
                ))
            })
        })
        .collect()
}

fn prepare_bodies<'a>(
    request: &BodyRequest<'_>,
    selected: &[NodeId],
    ingest: &'a IngestResult,
    interner: &'a Interner,
    source_texts: &'a [Cow<'a, str>],
) -> io::Result<Vec<PreparedBody<'a>>> {
    let mut bodies = Vec::with_capacity(selected.len());
    for (&id, source) in selected.iter().zip(source_texts) {
        let sym = &ingest.symbols[id as usize];
        let path = ingest.sym_file_path(sym, interner);
        let BodySelection { body, range } = body_range(source, request)?;
        let body = if request.outline {
            Cow::Owned(skeletonize_body(&body))
        } else {
            body
        };
        let body = if request.compress {
            Cow::Owned(crate::clones::token_stream::compress_source_body(&body))
        } else {
            body
        };
        bodies.push(PreparedBody {
            symbol: sym,
            name: interner.resolve(&sym.name),
            path,
            body,
            range,
        });
    }
    Ok(bodies)
}

pub fn execute_body_query<W: Write>(
    request: &BodyRequest<'_>,
    ingest: &IngestResult,
    interner: &Interner,
    sources: &dyn SourceReader,
    out: &mut W,
) -> io::Result<()> {
    let selected = select_symbols(request, ingest, interner)?;
    let files = read_source_files(&selected, ingest, interner, sources)?;
    let texts = source_texts(&selected, ingest, interner, &files)?;
    let bodies = prepare_bodies(request, &selected, ingest, interner, &texts)?;
    let items: Vec<BodyItem<'_>> = bodies
        .iter()
        .map(|body| BodyItem {
            symbol: body.symbol,
            name: body.name,
            path: body.path,
            body: &body.body,
            range: body.range,
        })
        .collect();
    crate::serialize::XmlReport::render_envelope(
        &BodyReport {
            root: request.root,
            items: &items,
        },
        out,
    )
}
