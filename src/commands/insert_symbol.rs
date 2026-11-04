//! `--insert-symbol SYM:TEXT --position before|after`: insert text beside a symbol's definition.
use super::common::edit::{EDIT_INS_PARAMS, InsertPosition, call_insert_tool, run_edit};
use super::common::mcp::mcp_tool;
use super::{Plugin, Run};
use clap::Args;
use std::io;

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct InsertSymbolArgs {
    /// Insert text beside a symbol definition, placed by --position
    #[arg(long = "insert-symbol", value_name = "SYM:TEXT")]
    pub insert_symbol: Option<String>,
    /// Where --insert-symbol puts its text
    #[arg(long, value_enum, value_name = "before|after")]
    pub position: Option<InsertPosition>,
}

pub static INSERT_SYMBOL: Plugin<InsertSymbolArgs> = Plugin {
    name: "insert-symbol",
    options: &[],
    selected: |args| args.insert_symbol.is_some(),
    run,
    check: Some(check),
    tools: &[mcp_tool!(
        "insert_symbol",
        "Atomically insert source code before or after a symbol definition (atomic write-rename)",
        EDIT_INS_PARAMS,
        &["symbol", "text", "position"],
        call_insert_tool
    )],
};

fn check(args: &InsertSymbolArgs) -> io::Result<()> {
    match (args.insert_symbol.is_some(), args.position.is_some()) {
        (true, false) => Err(io::Error::other(
            "ripwire-rs: --insert-symbol needs --position before|after",
        )),
        (false, true) => Err(io::Error::other(
            "ripwire-rs: --position needs --insert-symbol SYM:TEXT",
        )),
        _ => Ok(()),
    }
}

fn run(run: &Run<'_>, args: &InsertSymbolArgs) -> io::Result<()> {
    let (Some(spec), Some(position)) = (args.insert_symbol.as_deref(), args.position) else {
        return Err(io::Error::other(
            "ripwire-rs: --insert-symbol needs --position before|after",
        ));
    };
    run_edit(run, spec, position.op())
}
