//! `--external-surface`: names used but never defined in the index; honors `--limit` / `--offset`.
use super::{Plugin, Run, SharedOption};
use crate::commands::context::{ExecutionContext, Paginated};
use crate::domain::{IngestResult, Interner};
use crate::serialize::{PageRequest, XmlWriter};
use clap::Args;
use std::io::{self, Write};

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct ExternalSurfaceArgs {
    /// Names used but never defined in the index (external stdlib/deps surface)
    #[arg(long = "external-surface")]
    pub external_surface: bool,
    /// Include built-in functions in external-surface
    #[arg(long = "include-builtins")]
    pub include_builtins: bool,
}

pub fn execute_external_surface_query<W: Write>(
    root: &str,
    ingest: &IngestResult,
    interner: &Interner,
    page: PageRequest,
    out: &mut W,
) -> io::Result<()> {
    let by_name = crate::resolve::index_symbols_by_name(&ingest.symbols);
    let mut ext_counts: std::collections::HashMap<(lasso::Spur, crate::domain::Lang), usize> =
        std::collections::HashMap::new();

    for r in &ingest.references {
        let is_call = matches!(
            r.role,
            crate::domain::RefRole::Call | crate::domain::RefRole::Macro
        );
        let name = interner.resolve(&r.callee_name);
        if is_call
            && !r.callable_field
            && !r.macro_arg
            && !r.lang.semantics().is_prose_document()
            && !by_name.contains_key(&r.callee_name)
            && !r.lang.semantics().is_std_or_builtin_member(name)
            && !r.lang.semantics().is_builtin_keyword(name)
            && r.qualifier.is_none_or(|q| &interner[q] != "builtins")
        {
            *ext_counts.entry((r.callee_name, r.lang)).or_default() += 1;
        }
    }

    let mut items: Vec<(&str, &str, usize)> = ext_counts
        .iter()
        .map(|(&(n, l), &c)| (interner.resolve(&n), l.tag(), c))
        .collect();
    items.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(b.0)));

    crate::serialize::serialize_external_surface_xml(root, &items, page, out)
}

pub fn render<W: Write>(
    ctx: &ExecutionContext<'_>,
    _args: &ExternalSurfaceArgs,
    out: &mut XmlWriter<'_, W>,
) -> io::Result<()> {
    execute_external_surface_query(
        ctx.root,
        ctx.ingest,
        ctx.interner,
        ctx.cli.page_request(),
        out,
    )
}

fn run(run: &Run<'_>, args: &ExternalSurfaceArgs) -> io::Result<()> {
    run.report(|ctx, out| render(ctx, args, out))
}

pub static EXTERNAL_SURFACE: Plugin<ExternalSurfaceArgs> = Plugin {
    name: "external-surface",
    options: &[SharedOption::Paged],
    selected: |args| args.external_surface,
    run,
    check: None,
    tools: &[],
};
