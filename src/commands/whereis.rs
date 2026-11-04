//! `--whereis SYM`: which branches or trees define or mention a symbol.
use super::common::mcp::mcp_tool;
use super::{Plugin, Run, SharedOption};
use crate::commands::context::Paginated;
use crate::query::FileSources;
use crate::query::branch_tools::{
    WhereisRequest, validate_whereis_listing, whereis_with_sources_and_history,
};
use crate::query::commands::render_query_value;
use crate::query::doc_tools;
use crate::serialize::{JsonValueExt, serialize_whereis_xml};
use clap::Args;
use std::io;
use std::path::Path;

#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct WhereisArgs {
    /// Find which branches or trees define a symbol
    #[arg(long, value_name = "SYM")]
    pub whereis: Option<String>,
    /// Emit verbatim matched lines in --whereis without length truncation
    #[arg(long = "whereis-full-text")]
    pub whereis_full_text: bool,
    /// Select --whereis listing filter (defs|refs|all)
    #[arg(long = "whereis-listing", value_name = "MODE")]
    pub whereis_listing: Option<String>,
}

pub static WHEREIS: Plugin<WhereisArgs> = Plugin {
    name: "whereis",
    options: &[SharedOption::CliPaged],
    selected: |args| args.whereis.is_some(),
    run,
    check: None,
    tools: &[mcp_tool!(
        "whereis",
        "WHERE DOES THIS CONTENT LIVE: which file/branch defines or mentions a symbol",
        &[
            ("symbol", "string", "Symbol name"),
            ("with_history", "boolean", "Include repository history"),
            ("listing", "string", "Listing filter: defs, refs, or all"),
            ("full_text", "boolean", "Show full hit text"),
        ],
        &["symbol"],
        |s, a| {
            doc_tools::validate_whereis_listing(a.opt_str("listing"))?;
            let mut value = doc_tools::call_whereis(
                s.ingest,
                s.interner,
                s.root,
                a.req_str("symbol")?,
                a.bool_or("with_history", false),
            )?;
            for field in ["listing", "full_text"] {
                if let Some(selector) = a.get(field) {
                    value[field] = selector.clone();
                }
            }
            render_query_value(value, a, serialize_whereis_xml)
        }
    )],
};

fn run(run: &Run<'_>, args: &WhereisArgs) -> io::Result<()> {
    let sym = args.whereis.as_deref().unwrap_or_default();
    run.report(|ctx, out| {
        validate_whereis_listing(args.whereis_listing.as_deref()).map_err(io::Error::other)?;
        let sources = FileSources {
            root: Path::new(ctx.root),
        };
        let mut val = whereis_with_sources_and_history(
            WhereisRequest {
                root: ctx.root,
                symbol_name: sym,
                with_history: ctx.cli.modifiers.with_history,
            },
            &sources,
            ctx.ingest,
            ctx.interner,
        )
        .map_err(io::Error::other)?;
        if let Some(listing) = args.whereis_listing.as_deref() {
            val["listing"] = listing.into();
        }
        if args.whereis_full_text {
            val["full_text"] = true.into();
        }
        ctx.apply_pagination_to_json(&mut val);
        serialize_whereis_xml(&val, out)
    })
}
