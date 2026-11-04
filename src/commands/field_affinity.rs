//! `--field-affinity [STRUCT]`: cache-conscious field affinity; a named STRUCT adds its computed field offsets.
use super::plugin::report_command;
use crate::commands::context::ExecutionContext;
use crate::commands::context::Paginated;
use crate::serialize::XmlWriter;
use std::io::{self, Write};

report_command! {
    name: "field-affinity",
    args: FieldAffinityArgs,
    static: FIELD_AFFINITY,
    run: report(render),
    /// Cache-conscious field affinity; with STRUCT, its computed field offsets and padding
    #[arg(long = "field-affinity", num_args = 0..=1, default_missing_value = "", value_name = "STRUCT")]
    field_affinity: Option<Option<String>>,
}

pub fn render<W: Write>(
    ctx: &ExecutionContext<'_>,
    args: &FieldAffinityArgs,
    out: &mut XmlWriter<'_, W>,
) -> io::Result<()> {
    if let Some(sym_opt) = &args.field_affinity {
        let rep = crate::metrics::layout::compute_field_affinity_report(
            sym_opt.as_deref(),
            ctx.ingest,
            ctx.interner,
            ctx.root,
            ctx.cli.page_request(),
        );
        ctx.write_report(&rep, out)?;
    }
    Ok(())
}
