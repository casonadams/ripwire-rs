use crate::domain::{IngestResult, Interner};
use crate::query::SourceReader;
use serde_json::{Value, json};

pub fn mentions_from_sources(
    sources: &(impl SourceReader + ?Sized),
    ingest: &IngestResult,
    interner: &Interner,
    symbol_name: &str,
) -> Value {
    let mut matching_docs = Vec::new();
    let backtick_target = format!("`{symbol_name}`");

    for file in &ingest.files {
        let path = interner.resolve(&file.path);
        if file.lang.semantics().is_prose_document()
            && let Ok(bytes) = sources.read(path)
            && let Ok(content) = std::str::from_utf8(&bytes)
        {
            let occurrences = content.matches(&backtick_target).count();
            if occurrences > 0 {
                matching_docs.push(json!({
                    "file": path,
                    "mentions": occurrences
                }));
            }
        }
    }

    json!({
        "symbol": symbol_name,
        "docs": matching_docs,
        "total_mentions": matching_docs.iter().map(|d| d["mentions"].as_u64().unwrap_or(0)).sum::<u64>()
    })
}
