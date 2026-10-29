use crate::domain::{IngestResult, Interner, NodeId, RefRole, ScipSite, ScipTarget};
use lasso::Spur;
use protobuf::Message;
use scip::types::{Document, Index, SymbolRole};
use std::collections::{HashMap, HashSet};

/// Call sites a SCIP index pins: each indexed call reference whose site the index resolves to
/// one of our definitions or to a symbol it defines nowhere (another package).
///
/// # Errors
/// Returns the protobuf decode error when `bytes` is not a SCIP index.
pub fn scip_sites(
    bytes: &[u8],
    ingest: &IngestResult,
    interner: &Interner,
) -> Result<Vec<ScipSite>, protobuf::Error> {
    Ok(indexed_sites(bytes, ingest, interner)?.0)
}

/// A SCIP index answers only for files it holds documents for.
pub fn indexed_sites(
    bytes: &[u8],
    ingest: &IngestResult,
    interner: &Interner,
) -> Result<(Vec<ScipSite>, HashSet<u32>), protobuf::Error> {
    let index = Index::parse_from_bytes(bytes)?;
    let mut overlay = Overlay::new(ingest, interner);
    for doc in &index.documents {
        overlay.note_definitions(doc);
    }
    let mut sites = Vec::new();
    for doc in &index.documents {
        overlay.collect_sites(doc, &mut sites);
    }
    sites.sort_unstable_by_key(|s| (s.file_id, s.line, s.callee, s.target));
    sites.dedup_by_key(|s| (s.file_id, s.line, s.callee));
    let documented = index
        .documents
        .iter()
        .filter_map(|doc| overlay.file_ids.get(doc.relative_path.as_str()).copied())
        .collect();
    Ok((sites, documented))
}

fn is_definition(roles: i32) -> bool {
    roles & SymbolRole::Definition as i32 != 0
}

fn start_line(range: &[i32]) -> Option<u32> {
    let line = u32::try_from(*range.first()?).ok()?;
    Some(line + 1)
}

/// Line-sorted definitions per `(file, name)`: a SCIP definition sits on the identifier, at or
/// after the line our definition node starts on (decorators, multi-line signatures).
type ByName<T> = HashMap<(u32, Spur), Vec<(u32, T)>>;

fn nearest_at_or_before<T: Copy>(
    index: &ByName<T>,
    file_id: u32,
    name: Spur,
    line: u32,
) -> Option<T> {
    let entries = index.get(&(file_id, name))?;
    let upper = entries.partition_point(|&(l, _)| l <= line);
    entries[..upper].last().map(|&(_, value)| value)
}

fn by_name<T: Copy + Ord>(entries: impl Iterator<Item = (u32, Spur, u32, T)>) -> ByName<T> {
    let mut index: ByName<T> = HashMap::new();
    for (file_id, name, line, value) in entries {
        index
            .entry((file_id, name))
            .or_default()
            .push((line, value));
    }
    for list in index.values_mut() {
        list.sort_unstable();
        list.dedup();
    }
    index
}

struct Overlay<'a> {
    interner: &'a Interner,
    file_ids: HashMap<&'a str, u32>,
    defs: ByName<NodeId>,
    calls: HashSet<(u32, u32, Spur)>,
    names: HashMap<String, Option<Spur>>,
    defined: HashSet<String>,
    pinned: HashMap<String, NodeId>,
}

impl<'a> Overlay<'a> {
    fn new(ingest: &'a IngestResult, interner: &'a Interner) -> Self {
        Self {
            interner,
            file_ids: ingest
                .files
                .iter()
                .map(|f| (interner.resolve(&f.path), f.id))
                .collect(),
            defs: by_name(
                ingest
                    .symbols
                    .iter()
                    .map(|s| (s.file_id, s.name, s.line, s.id)),
            ),
            calls: ingest
                .references
                .iter()
                .filter(|r| matches!(r.role, RefRole::Call | RefRole::Macro | RefRole::Access))
                .map(|r| (r.file_id, r.name_line, r.callee_name))
                .collect(),
            names: HashMap::new(),
            defined: HashSet::new(),
            pinned: HashMap::new(),
        }
    }

    /// The interned leaf name of a global SCIP symbol (`pkg/Type#method().` -> `method`).
    fn leaf_name(&mut self, symbol: &str) -> Option<Spur> {
        if let Some(&name) = self.names.get(symbol) {
            return name;
        }
        let name = scip::symbol::is_global_symbol(symbol)
            .then(|| scip::symbol::parse_symbol(symbol).ok())
            .flatten()
            .and_then(|parsed| parsed.descriptors.last().map(|d| d.name.clone()))
            .and_then(|leaf| self.interner.get(&leaf));
        self.names.insert(symbol.to_string(), name);
        name
    }

    fn note_definitions(&mut self, doc: &Document) {
        let file_id = self.file_ids.get(doc.relative_path.as_str()).copied();
        for occ in doc
            .occurrences
            .iter()
            .filter(|o| is_definition(o.symbol_roles))
        {
            self.defined.insert(occ.symbol.clone());
            let (Some(file_id), Some(line)) = (file_id, start_line(&occ.range)) else {
                continue;
            };
            let Some(name) = self.leaf_name(&occ.symbol) else {
                continue;
            };
            if let Some(id) = nearest_at_or_before(&self.defs, file_id, name, line) {
                self.pinned.insert(occ.symbol.clone(), id);
            }
        }
    }

    fn collect_sites(&mut self, doc: &Document, sites: &mut Vec<ScipSite>) {
        let Some(&file_id) = self.file_ids.get(doc.relative_path.as_str()) else {
            return;
        };
        for occ in doc
            .occurrences
            .iter()
            .filter(|o| !is_definition(o.symbol_roles))
        {
            let (Some(line), Some(callee)) = (start_line(&occ.range), self.leaf_name(&occ.symbol))
            else {
                continue;
            };
            if !self.calls.contains(&(file_id, line, callee)) {
                continue;
            }
            let target = match self.pinned.get(&occ.symbol) {
                Some(&id) => ScipTarget::Def(id),
                None if !self.defined.contains(&occ.symbol) => ScipTarget::External,
                None => continue,
            };
            sites.push(ScipSite {
                file_id,
                line,
                callee,
                target,
            });
        }
    }
}
