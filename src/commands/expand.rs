//! `--expand SYMS`: full symbol bodies, or their control-flow skeletons with `--outline`; `--compress` selects the compact dialect.
use super::{Plugin, Run};
use crate::commands::common::ladder::execute_expand;
use crate::commands::context::ExecutionContext;
use crate::serialize::XmlWriter;
use clap::Args;
use std::io::{self, Write};

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct ExpandArgs {
    /// Expand full symbol bodies and callee signatures (SYM,SYM...)
    #[arg(long, value_name = "SYMS")]
    pub expand: Option<String>,
    /// With --expand: show control-flow skeletons (declarations, loops, branches) instead of full bodies
    #[arg(long, requires = "expand")]
    pub outline: bool,
}

pub fn render(
    ctx: &ExecutionContext<'_>,
    args: &ExpandArgs,
    out: &mut XmlWriter<'_, impl Write>,
) -> io::Result<()> {
    execute_expand(
        ctx.root,
        args.expand.as_deref().unwrap_or_default(),
        args.outline,
        ctx.cli.modifiers.compress,
        ctx.ingest,
        ctx.interner,
        out,
    )
}

fn run(run: &Run<'_>, args: &ExpandArgs) -> io::Result<()> {
    run.report(|ctx, out| render(ctx, args, out))
}

pub static EXPAND: Plugin<ExpandArgs> = Plugin {
    name: "expand",
    options: &[],
    selected: |args| args.expand.is_some(),
    run,
    check: None,
    tools: &[],
};
