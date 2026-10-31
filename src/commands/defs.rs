//! `--defs A,B,...`: the definitions each name binds, each with a selector that pins it.
use super::common::mcp::mcp_tool;
use super::common::retrieval::report_query;
use super::{Plugin, Run};
use crate::query::commands::{CommandDescriptor, QueryContext, render_xml};
use crate::query::defs::{DefsRequest, execute_defs, split_defs_names};
use clap::Args;
use std::io::{self, Write};

/// `--defs`.
#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct DefsArgs {
    /// Definitions bound by up to 16 names (A,B,...), each with a selector that pins it
    #[arg(long, value_name = "A,B")]
    pub defs: Option<String>,
}

pub fn execute<W: Write>(query: &QueryContext<'_>, names: &str, out: &mut W) -> io::Result<()> {
    let names = split_defs_names(names);
    let request = DefsRequest {
        root: query.root,
        names: &names,
    };
    execute_defs(&request, query.ingest, query.graph, query.interner, out)
}

fn run(run: &Run<'_>, args: &DefsArgs) -> io::Result<()> {
    let names = args.defs.as_deref().unwrap_or_default();
    report_query(run, |query, out| execute(query, names, out))
}

pub static DEFS: Plugin<DefsArgs> = Plugin {
    name: "defs",
    options: &[],
    selected: |args| args.defs.is_some(),
    run,
    check: None,
    tools: TOOLS,
};

static TOOLS: &[&CommandDescriptor] = &[mcp_tool!(
    "defs",
    "Which definitions up to 16 names bind: owner, kind, file:line, language, test or source, caller count, and a sel= spelling that pins each one",
    &[("symbols", "string[]", "List of symbol names")],
    &["symbols"],
    |s, a| {
        let symbols: Vec<&str> = a["symbols"]
            .as_array()
            .ok_or("symbols must be an array")?
            .iter()
            .map(|name| name.as_str().ok_or("symbol names must be strings"))
            .collect::<Result<_, _>>()?;
        let request = DefsRequest {
            root: s.root,
            names: &symbols,
        };
        render_xml(|out| execute_defs(&request, s.ingest, s.graph, s.interner, out))
    }
)];
