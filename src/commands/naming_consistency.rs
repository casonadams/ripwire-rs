//! `--naming-consistency`: naming convention consistency check over symbols and locals.
use super::SharedOption;
use super::plugin::report_command;
use crate::commands::context::{ExecutionContext, Paginated};
use crate::serialize::XmlWriter;
use std::io::{self, Write};

report_command! {
    name: "naming-consistency",
    args: NamingConsistencyArgs,
    static: NAMING_CONSISTENCY,
    options: [SharedOption::Paged],
    run: report(render),
    /// Naming convention consistency check over symbols and locals
    #[arg(long = "naming-consistency")]
    naming_consistency: bool,
}

pub fn render<W: Write>(
    ctx: &ExecutionContext<'_>,
    _args: &NamingConsistencyArgs,
    out: &mut XmlWriter<'_, W>,
) -> io::Result<()> {
    crate::metrics::serialize_naming_consistency_xml(
        ctx.root,
        ctx.ingest,
        ctx.interner,
        ctx.cli.page_request(),
        out,
    )
}
