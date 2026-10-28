//! Identifier-use counts across a corpus: name-use evidence for dead-code checks.
use crate::domain::{IngestResult, Interner, Lang};
use std::collections::HashMap;

/// Parse `source` with `lang`'s grammar for `path`, on this thread's shared parser.
pub(crate) fn parse_file_tree(lang: Lang, path: &str, source: &[u8]) -> Option<tree_sitter::Tree> {
    let grammar = lang.semantics().source_grammar(path)?;
    crate::ingest::registry::PARSER_STATE.with(|state| {
        let mut borrowed = state.borrow_mut();
        let (parser, _) = &mut *borrowed;
        parser.set_language(grammar).ok()?;
        parser.parse(source, None)
    })
}

/// Adds to each name in `uses` the identifier tokens of every `lang` file that use that name;
/// definitions and prototypes do not count, so any count is evidence of a reference.
pub fn count_identifier_uses<'a>(
    ingest: &IngestResult,
    interner: &Interner,
    sources: &dyn crate::query::SourceReader,
    lang: Lang,
    uses: &mut HashMap<&'a str, u32>,
) {
    use rayon::prelude::*;
    let files: Vec<_> = ingest
        .files
        .iter()
        .filter(|file| file.lang == lang)
        .collect();
    let known = &*uses;
    let counted: HashMap<&'a str, u32> = files
        .par_iter()
        .fold(HashMap::new, |mut local, file| {
            let path = ingest.file_path(file.id, interner);
            let source = sources
                .read(path)
                .unwrap_or(std::borrow::Cow::Borrowed(&[]));
            if let Some(tree) = parse_file_tree(lang, path, source.as_ref()) {
                lang.semantics().for_each_identifier(
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
