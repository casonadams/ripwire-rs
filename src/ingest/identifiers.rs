//! Identifier-use counts across a corpus: name-use evidence for dead-code checks.
use crate::domain::{IngestResult, Interner, Lang};
use std::collections::HashMap;

/// Parse `source` with `lang`'s grammar for `path`, on this thread's shared parser.
pub(crate) fn parse_file_tree(lang: Lang, path: &str, source: &[u8]) -> Option<tree_sitter::Tree> {
    let grammar = lang.semantics().source_grammar(path)?;
    crate::ingest::registry::parse_with_grammar(grammar, source)
}

/// Adds to each name in `uses` the identifier tokens of every `lang` file that use that name;
/// definitions and prototypes do not count, so any count is evidence of a reference.
pub fn count_identifier_uses(
    ingest: &IngestResult,
    interner: &Interner,
    sources: &dyn crate::query::SourceReader,
    lang: Lang,
    uses: &mut HashMap<&str, u32>,
) {
    let files: Vec<_> = ingest
        .files
        .iter()
        .filter(|file| file.lang == lang)
        .collect();
    count_uses_in_files(ingest, interner, sources, &files, uses);
}

/// [`count_identifier_uses`] over the non-test, non-bench files only: a use here is a reference from
/// production code.
pub fn count_production_identifier_uses(
    ingest: &IngestResult,
    interner: &Interner,
    sources: &dyn crate::query::SourceReader,
    lang: Lang,
    uses: &mut HashMap<&str, u32>,
) {
    let files: Vec<_> = ingest
        .files
        .iter()
        .filter(|file| file.lang == lang)
        .filter(|file| {
            !crate::domain::test_path::is_test_or_bench_path(ingest.file_path(file.id, interner))
        })
        .collect();
    count_uses_in_files(ingest, interner, sources, &files, uses);
}

fn count_uses_in_files<'a>(
    ingest: &IngestResult,
    interner: &Interner,
    sources: &dyn crate::query::SourceReader,
    files: &[&crate::domain::FileInfo],
    uses: &mut HashMap<&'a str, u32>,
) {
    use rayon::prelude::*;
    let known = &*uses;
    let counted: HashMap<&'a str, u32> = files
        .par_iter()
        .fold(HashMap::new, |mut local, file| {
            let path = ingest.file_path(file.id, interner);
            let source = sources
                .read(path)
                .unwrap_or(std::borrow::Cow::Borrowed(&[]));
            if let Some(tree) = parse_file_tree(file.lang, path, source.as_ref()) {
                file.lang.semantics().for_each_identifier(
                    tree.root_node(),
                    source.as_ref(),
                    &mut |text| {
                        if let Some((&name, _)) = known.get_key_value(text) {
                            *local.entry(name).or_insert(0) += 1;
                        }
                    },
                );
            }
            local
        })
        .reduce(HashMap::new, |mut left, right| {
            for (name, count) in right {
                *left.entry(name).or_insert(0) += count;
            }
            left
        });
    for (name, count) in counted {
        if let Some(total) = uses.get_mut(name) {
            *total += count;
        }
    }
}

/// The files of `lang` that use each of `names` as an identifier; definitions and prototypes do not count.
pub fn identifier_use_files(
    ingest: &IngestResult,
    interner: &Interner,
    sources: &dyn crate::query::SourceReader,
    lang: Lang,
    names: &std::collections::HashSet<&str>,
) -> HashMap<String, Vec<u32>> {
    use rayon::prelude::*;
    let files: Vec<_> = ingest
        .files
        .iter()
        .filter(|file| file.lang == lang)
        .collect();
    let per_file: Vec<(u32, Vec<String>)> = files
        .par_iter()
        .map(|file| {
            let path = ingest.file_path(file.id, interner);
            let source = sources
                .read(path)
                .unwrap_or(std::borrow::Cow::Borrowed(&[]));
            let mut found = std::collections::HashSet::new();
            if let Some(tree) = parse_file_tree(file.lang, path, source.as_ref()) {
                file.lang.semantics().for_each_identifier(
                    tree.root_node(),
                    source.as_ref(),
                    &mut |text| {
                        if names.contains(text) {
                            found.insert(text.to_string());
                        }
                    },
                );
            }
            (file.id, found.into_iter().collect())
        })
        .collect();
    let mut by_name: HashMap<String, Vec<u32>> = HashMap::new();
    for (file, found) in per_file {
        for name in found {
            by_name.entry(name).or_default().push(file);
        }
    }
    by_name
}
