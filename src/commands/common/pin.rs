//! Pins the quality baseline sidecar for `--baseline`.

use crate::query::safety::SafetyContext;
use std::io::{self, Write};
use std::path::Path;

pub fn pin_quality_baseline<W: Write>(ctx: &SafetyContext<'_>, out: &mut W) -> io::Result<()> {
    let SafetyContext {
        root,
        ingest,
        graph,
        interner,
    } = *ctx;
    let head_sha = crate::git::history::compute_git_at_stamp(root)
        .map_err(io::Error::other)?
        .unwrap_or_default();
    let short_sha = &head_sha[..9.min(head_sha.len())];
    let snap = crate::gate::baseline::compute_snapshot(ingest, graph, interner, short_sha);
    let baseline_path = Path::new(root).join(crate::gate::baseline::DEFAULT_BASELINE_FILE);
    crate::gate::baseline::write_baseline_file(&snap, &baseline_path)?;
    writeln!(
        out,
        "ripwire-rs: pinned quality baseline stamped at {short_sha} -> {}",
        baseline_path.display()
    )
}
