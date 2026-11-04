use ripwire_rs::domain::CorpusView;
use ripwire_rs::{IngestResult, Interner};
use serde_json::{Value, json};

pub fn uses_json(ingest: &IngestResult, interner: &Interner, symbol: &str) -> Value {
    let sites: Vec<Value> = (ingest, interner)
        .collect_symbol_uses(symbol)
        .into_iter()
        .map(|(role, file, line, enclosing)| {
            json!({"role": role, "file": file, "line": line, "enclosing": enclosing})
        })
        .collect();
    json!({"symbol": symbol, "count": sites.len(), "uses": sites})
}
