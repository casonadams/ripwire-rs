//! `--pack-top-n N`: bodies of the top N highest-ranked symbols.
use super::plugin::report_command;
use crate::commands::context::ExecutionContext;
use crate::domain::{IngestResult, Interner};
use crate::rank::PageRankResult;
use crate::serialize::XmlWriter;
use std::io::{self, Write};

report_command! {
    name: "pack-top-n",
    args: PackTopNArgs,
    static: PACK_TOP_N,
    run: report(render),
    /// Pack bodies of the top N highest-ranked symbols
    #[arg(long = "pack-top-n", value_name = "N")]
    pack_top_n: Option<usize>,
}

fn execute_pack_top_n_query<W: Write>(
    root: &str,
    n: usize,
    ingest: &IngestResult,
    pr: &PageRankResult,
    interner: &Interner,
    out: &mut W,
) -> io::Result<()> {
    let (file_order, buckets) = crate::serialize::bucket_symbols_by_rank(
        &ingest.symbols,
        ingest.files.len(),
        &pr.ranks,
        Some(n),
    );
    let mut bodies = Vec::new();
    let mut count = 0;

    for f in file_order {
        if count >= n {
            break;
        }
        let rel_path = ingest.file_path(f, interner);
        if let Ok(bytes) = crate::crawler::read_rel_bytes(root, rel_path) {
            let content = String::from_utf8_lossy(&bytes).to_string();
            bodies.push((rel_path, content));
            count += buckets[f as usize].len();
        }
    }

    let bodies_slice: Vec<(&str, &str)> = bodies.iter().map(|(p, c)| (*p, c.as_str())).collect();
    crate::serialize::serialize_pack_top_n_xml(root, &bodies_slice, out)
}

pub fn render<W: Write>(
    ctx: &ExecutionContext<'_>,
    args: &PackTopNArgs,
    out: &mut XmlWriter<'_, W>,
) -> io::Result<()> {
    execute_pack_top_n_query(
        ctx.root,
        args.pack_top_n.unwrap_or_default(),
        ctx.ingest,
        ctx.pr,
        ctx.interner,
        out,
    )
}
