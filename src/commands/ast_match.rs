//! `--match QUERY`: AST pattern query over the corpus; honors `--limit/--offset`.
use super::common::grep::write_hits;
use super::common::retrieval::report_text_query;
use super::{Plugin, Run, SharedOption};
use crate::query::commands::QueryContext;
use crate::query::retrieval;
use crate::serialize::PageRequest;
use clap::Args;
use std::io::{self, Write};

/// `--match`.
#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct AstMatchArgs {
    /// AST pattern query expression
    #[arg(long = "match", value_name = "QUERY")]
    pub ast_match: Option<String>,
}

pub fn execute<W: Write>(query: &QueryContext<'_>, expr: &str, page: PageRequest, out: &mut W) -> io::Result<()> {
    let val = retrieval::call_ast_match(query.sources, query.ingest, query.interner, expr)
        .map_err(io::Error::other)?;
    write_hits(val, page, out)
}

fn run(run: &Run<'_>, args: &AstMatchArgs) -> io::Result<()> {
    report_text_query(run, args.ast_match.as_deref(), |query, expr, page, out| {
        execute(query, expr, page, out)
    })
}

pub static AST_MATCH: Plugin<AstMatchArgs> = Plugin {
    name: "match",
    options: &[SharedOption::Paged],
    selected: |args| args.ast_match.is_some(),
    run,
    check: None,
    tools: &[],
};
