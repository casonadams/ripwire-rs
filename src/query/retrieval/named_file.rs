use crate::domain::{IngestResult, Interner, Symbol};

/// An indexed file that a `--for` task names by its whole path, with that file's symbols ranked.
pub(super) struct NamedFile<'a> {
    pub path: &'a str,
    pub scored: Vec<(f64, &'a Symbol)>,
}

/// The file `task` names exactly (quotes, backticks and a leading `./` aside), or `None`.
pub(super) fn named_file<'a>(
    ingest: &'a IngestResult,
    interner: &'a Interner,
    ranks: &[f64],
    task: &str,
) -> Option<NamedFile<'a>> {
    let wanted = task
        .trim()
        .trim_matches(|c| matches!(c, '`' | '"' | '\''))
        .trim_start_matches("./");
    let file = ingest
        .files
        .iter()
        .find(|f| interner.resolve(&f.path) == wanted)?;
    let mut scored: Vec<(f64, &Symbol)> = ingest
        .symbols
        .iter()
        .filter(|s| s.file_id == file.id)
        .map(|s| (ranks.get(s.id as usize).copied().unwrap_or(0.0), s))
        .collect();
    scored.sort_by(|a, b| {
        b.0.partial_cmp(&a.0)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then(a.1.line.cmp(&b.1.line))
    });
    Some(NamedFile {
        path: interner.resolve(&file.path),
        scored,
    })
}
