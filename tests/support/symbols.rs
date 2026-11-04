use lasso::Spur;
use ripwire_rs::{IngestResult, Interner, NodeId, Symbol};

/// The first symbol whose `field` resolves to `name`; panics when none does.
pub fn find_symbol<'a>(
    ingest: &'a IngestResult,
    interner: &Interner,
    field: impl Fn(&Symbol) -> Spur,
    name: &str,
) -> &'a Symbol {
    ingest
        .symbols
        .iter()
        .find(|sym| interner.resolve(&field(sym)) == name)
        .unwrap_or_else(|| panic!("symbol {name} not indexed"))
}

/// The `field` names of the symbols `ids` name, sorted.
pub fn sorted_names(
    ingest: &IngestResult,
    interner: &Interner,
    ids: impl Iterator<Item = NodeId>,
    field: impl Fn(&Symbol) -> Spur,
) -> Vec<String> {
    let mut names: Vec<String> = ids
        .map(|id| {
            interner
                .resolve(&field(ingest.sym(id).expect("symbol")))
                .to_string()
        })
        .collect();
    names.sort();
    names
}
