pub mod detector;
pub mod token_stream;

pub use detector::*;
pub use token_stream::*;

use std::io::{self, Write};
use std::path::Path;

/// Default minimum token length for clone grouping.
pub const DEFAULT_MIN_CLONE_TOKENS: usize = 40;

/// Execute clone detection on the target repository and print results.
pub fn execute_clones_command(
    root: &str,
    min_tokens: usize,
    page: crate::serialize::PageRequest,
) -> io::Result<()> {
    let root_path = Path::new(root);
    let (ingest, _graph, _pr, interner) = crate::workspace::load_repo_analysis(root)?;
    let clones = find_clones(&ingest, &interner, min_tokens, CloneSource::Root(root_path));
    let xml = render_clones_xml(&clones, &ingest, &interner, page);
    let mut stdout_lock = io::stdout().lock();
    let mut out = crate::serialize::XmlWriter::new(&mut stdout_lock, false);
    out.write_all(xml.as_bytes())?;
    out.finish()
}
