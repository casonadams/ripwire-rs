mod buckets;
mod inventory;
mod options;
mod root;
mod rows;
mod stream;

use crate::domain::{IngestResult, Interner};
use crate::graph::Graph;
use crate::rank::PageRankResult;

pub use buckets::bucket_symbols_by_rank;
pub use options::{DEFAULT_PACK_BUDGET_BYTES, DEFAULT_STREAM_BUFFER_CAPACITY, SerializeOptions};
pub use root::{RootElementContext, write_root_open, write_root_open_with_rank_by};
pub use rows::{write_file_header, write_symbol_row};
pub use stream::{CountingWriter, StreamXmlContext, serialize_xml, serialize_xml_with_ranks};

/// Serialize to a minified XML string in memory (primarily for unit tests and inspection).
#[must_use]
pub fn serialize_xml_to_string(
    ingest: &IngestResult,
    graph: &Graph,
    pr: &PageRankResult,
    interner: &Interner,
    options: &SerializeOptions,
) -> String {
    super::render_string(|out| serialize_xml(ingest, graph, pr, interner, out, options))
}
