//! `--zoom [DEPTH]`: multi-level hierarchical Leiden clustering; honors `--limit` / `--offset`.
use super::{Plugin, Run, SharedOption};
use crate::commands::context::{ExecutionContext, Paginated};
use clap::Args;
use std::io::{self, Write};

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct ZoomArgs {
    /// Multi-level hierarchical Leiden community clustering
    #[arg(long = "zoom", value_name = "DEPTH")]
    pub zoom: Option<Option<u32>>,
    /// Max depth levels for --zoom
    #[arg(long = "zoom-levels", value_name = "N")]
    pub zoom_levels: Option<u32>,
}

pub(crate) fn validate_zoom(a: &ZoomArgs) -> io::Result<()> {
    if a.zoom_levels.is_some() && a.zoom.is_none() {
        return Err(io::Error::other(
            "ripwire-rs: --zoom-levels=N modifies --zoom; pass it too (e.g. --zoom --zoom-levels=0 prints every level)",
        ));
    }
    Ok(())
}

pub fn render<W: Write>(
    ctx: &ExecutionContext<'_>,
    args: &ZoomArgs,
    out: &mut crate::serialize::XmlWriter<'_, W>,
) -> io::Result<()> {
    let page = ctx.cli.page_request();
    let opts = crate::navigate::zoom::ZoomOptions {
        depth: args.zoom.flatten(),
        levels_shown: args.zoom_levels,
        page,
    };
    crate::navigate::zoom::execute_zoom_query(
        ctx.ingest,
        ctx.graph,
        ctx.interner,
        ctx.pr.iterations,
        opts,
        out,
    )
}

fn run(run: &Run<'_>, args: &ZoomArgs) -> io::Result<()> {
    run.report(|ctx, out| render(ctx, args, out))
}

pub static ZOOM: Plugin<ZoomArgs> = Plugin {
    name: "zoom",
    options: &[SharedOption::Paged],
    selected: |args| args.zoom.is_some(),
    run,
    check: Some(validate_zoom),
    tools: &[],
};
