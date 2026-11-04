use ripwire_rs::{FileInfo, IngestResult, Interner, Lang, SymKind, Symbol};

/// An in-memory ingest with one Rust file at `path` holding one ten-line function `name`.
pub fn one_function_ingest(
    interner: &Interner,
    path: &str,
    name: &str,
    line_count: u32,
    byte_size: u64,
) -> IngestResult {
    let mut ingest = IngestResult::new();
    ingest.files.push(FileInfo {
        id: 0,
        path: interner.intern(path),
        lang: Lang::Rust,
        line_count,
        byte_size,
    });
    let name = interner.intern(name);
    let mut sym = Symbol::new(0, name, name, SymKind::Function, Lang::Rust, 0, 10);
    sym.loc = 10;
    ingest.symbols.push(sym);
    ingest
}
