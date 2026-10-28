use super::{Plugin, Run};
use crate::commands::context::crawler_options_for_cli;
use crate::serialize::escape::escape_xml_str;
use clap::Args;
use std::io::{self, Write};
use std::path::Path;

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct IndexOutArgs {
    /// Cold-parse the tree, write the index cache to FILE (reload with --cache=FILE), and exit
    #[arg(long = "index-out", value_name = "FILE")]
    pub index_out: Option<String>,
}

fn run(run: &Run<'_>, args: &IndexOutArgs) -> io::Result<()> {
    let path = args.index_out.as_deref().unwrap_or_default();
    let opts = crawler_options_for_cli(run.cli);
    let scip = run.cli.scip.as_deref().map(Path::new);
    if !crate::workspace::is_analysis_cacheable(&opts, scip) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "ripwire-rs: --index-out writes a cache --cache can reload, so it refuses --exclude, --no-ignore, --ignore-tests, --max-file-size and --scip",
        ));
    }
    let (ingest, graph, pr, interner) =
        crate::workspace::load_repo_analysis_with_options(run.root, None, true, &opts, None)?;
    crate::cache::save_cached_analysis(
        run.root,
        Some(Path::new(path)),
        "",
        (&ingest, &graph, &pr, &interner),
    )?;
    writeln!(
        io::stdout().lock(),
        "<index-out schema=\"ripwire-rs.index-out/v1\" root=\"{}\" file=\"{}\" files=\"{}\" symbols=\"{}\" edges=\"{}\" key=\"{}\" next=\"--cache={}\"/>",
        escape_xml_str(run.root),
        escape_xml_str(path),
        ingest.files.len(),
        ingest.symbols.len(),
        graph.edge_count(),
        escape_xml_str(&crate::cache::compute_cache_key(run.root)),
        escape_xml_str(path),
    )
}

pub static INDEX_OUT: Plugin<IndexOutArgs> = Plugin {
    name: "index-out",
    options: &[],
    selected: |args| args.index_out.is_some(),
    run,
    check: None,
    tools: &[],
};
