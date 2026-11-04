//! Body query behind `--expand`: full bodies, or control-flow skeletons when `outline` is set.
use crate::domain::{IngestResult, Interner};
use std::io::{self, Write};
use std::path::Path;

pub fn execute_expand<W: Write>(
    root: &str,
    syms_spec: &str,
    outline: bool,
    compress: bool,
    ingest: &IngestResult,
    interner: &Interner,
    out: &mut W,
) -> io::Result<()> {
    let sources = crate::query::FileSources {
        root: Path::new(root),
    };
    let request = crate::query::detail::BodyRequest {
        root,
        symbols: syms_spec,
        path_hint: None,
        start_line: None,
        end_line: None,
        outline,
        compress,
    };
    crate::query::detail::execute_body_query(&request, ingest, interner, &sources, out)
}
