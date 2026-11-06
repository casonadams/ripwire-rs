use crate::domain::{IngestResult, Interner, NodeId, RefRole, ScipSite, ScipTarget};
use lasso::Spur;
use protobuf::Message;
use scip::types::occurrence::Typed_range;
use scip::types::{Document, Index, Occurrence, SymbolRole};
use std::collections::{HashMap, HashSet};
use std::fmt;

/// Why a SCIP index cannot score call sites.
#[derive(Debug)]
pub enum ScipIndexError {
    /// The bytes are not a SCIP index.
    Decode(protobuf::Error),
    /// The index holds occurrences and none carries a readable range, so every site would drop
    /// silently and the index would score as zero recall.
    NoDecodableRanges { occurrences: usize },
}

impl fmt::Display for ScipIndexError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(e) => write!(f, "not a SCIP index: {e}"),
            Self::NoDecodableRanges { occurrences } => write!(
                f,
                "SCIP index has {occurrences} occurrences and none has a readable range (neither typed_range nor range)"
            ),
        }
    }
}

impl std::error::Error for ScipIndexError {}

impl From<protobuf::Error> for ScipIndexError {
    fn from(e: protobuf::Error) -> Self {
        Self::Decode(e)
    }
}

/// Call sites a SCIP index pins: each indexed call reference whose site the index resolves to
/// one of our definitions or to a symbol it defines nowhere (another package).
///
/// # Errors
/// Returns `Decode` when `bytes` is not a SCIP index, and `NoDecodableRanges` when no occurrence
/// has a readable range.
pub fn scip_sites(
    bytes: &[u8],
    ingest: &IngestResult,
    interner: &Interner,
) -> Result<Vec<ScipSite>, ScipIndexError> {
    Ok(indexed_sites(bytes, ingest, interner)?.0)
}

/// A SCIP index answers only for files it holds documents for.
pub fn indexed_sites(
    bytes: &[u8],
    ingest: &IngestResult,
    interner: &Interner,
) -> Result<(Vec<ScipSite>, HashSet<u32>), ScipIndexError> {
    let index = Index::parse_from_bytes(bytes)?;
    check_ranges(&index)?;
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

/// Fails when the index has occurrences and none yields a line, so no site could be pinned.
fn check_ranges(index: &Index) -> Result<(), ScipIndexError> {
    let (mut occurrences, mut readable) = (0usize, 0usize);
    for occ in index.documents.iter().flat_map(|doc| &doc.occurrences) {
        occurrences += 1;
        readable += usize::from(occurrence_line(occ).is_some());
    }
    if occurrences > 0 && readable == 0 {
        return Err(ScipIndexError::NoDecodableRanges { occurrences });
    }
    Ok(())
}

fn is_definition(roles: i32) -> bool {
    roles & SymbolRole::Definition as i32 != 0
}

/// The 1-based start line of an occurrence. Indexers write either the `typed_range` oneof (what
/// scip-java 0.13 and later write, and what the protocol says new producers must write) or the
/// deprecated packed `range`; `typed_range` wins when both are present.
fn occurrence_line(occ: &Occurrence) -> Option<u32> {
    let line = match &occ.typed_range {
        Some(Typed_range::SingleLineRange(r)) => r.line,
        Some(Typed_range::MultiLineRange(r)) => r.start_line,
        _ => *occ.range.first()?,
    };
    Some(u32::try_from(line).ok()? + 1)
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

/// The definition a reference names when its symbol string has one definition, or exactly one in
/// the reference's own file. A string shared by several definitions elsewhere names none of them.
fn pinned_target(defs: &[(u32, NodeId)], file_id: u32) -> Option<NodeId> {
    match defs {
        [(_, id)] => Some(*id),
        _ => {
            let mut same_file = defs.iter().filter(|(def_file, _)| *def_file == file_id);
            match (same_file.next(), same_file.next()) {
                (Some(&(_, id)), None) => Some(id),
                _ => None,
            }
        }
    }
}

struct Overlay<'a> {
    interner: &'a Interner,
    file_ids: HashMap<&'a str, u32>,
    defs: ByName<NodeId>,
    calls: HashSet<(u32, u32, Spur)>,
    names: HashMap<String, Option<Spur>>,
    defined: HashSet<String>,
    /// Every definition a symbol string has, with the file that defines it. rust-analyzer gives a
    /// library item and a same-named item in a test crate one symbol string, so a string can name
    /// several of our definitions.
    pinned: HashMap<String, Vec<(u32, NodeId)>>,
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
            let (Some(file_id), Some(line)) = (file_id, occurrence_line(occ)) else {
                continue;
            };
            let Some(name) = self.leaf_name(&occ.symbol) else {
                continue;
            };
            if let Some(id) = nearest_at_or_before(&self.defs, file_id, name, line) {
                let entry = self.pinned.entry(occ.symbol.clone()).or_default();
                if !entry.contains(&(file_id, id)) {
                    entry.push((file_id, id));
                }
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
            let (Some(line), Some(callee)) = (occurrence_line(occ), self.leaf_name(&occ.symbol))
            else {
                continue;
            };
            if !self.calls.contains(&(file_id, line, callee)) {
                continue;
            }
            let target = match self.pinned.get(&occ.symbol) {
                Some(defs) => match pinned_target(defs, file_id) {
                    Some(id) => ScipTarget::Def(id),
                    None => continue,
                },
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
