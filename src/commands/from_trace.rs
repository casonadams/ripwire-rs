//! `--from-trace FILE|TEXT|-`: maps a stack trace or compiler error onto indexed symbols, innermost first.
use super::common::mcp::mcp_tool;
use super::plugin::report_command;
use crate::query::commands::{QueryContext, render_xml};
use crate::query::graph as graph_tools;
use crate::serialize::JsonValueExt;
use std::io::{self, Write};

/// `-` reads stdin, text with a newline or `:` is used as-is, and any other value is read as a file when it exists.
fn trace_text(query: &QueryContext<'_>, tr: &str) -> io::Result<String> {
    if tr == "-" {
        stdin_trace(io::stdin())
    } else if tr.contains('\n') || tr.contains(':') {
        Ok(tr.to_string())
    } else {
        trace_from_file_read(tr, query.sources.read(tr).map(|bytes| bytes.into_owned()))
    }
}

/// The trace text for an argument read as a file: the file's text, or the argument itself only when no
/// file has that name. Any other read failure is an error, never a trace.
pub fn trace_from_file_read(arg: &str, read: io::Result<Vec<u8>>) -> io::Result<String> {
    crate::crawler::or_absent(read.map(|bytes| String::from_utf8_lossy(&bytes).into_owned()), || {
        arg.to_string()
    })
}

/// The whole of a stdin stream as trace text; a read failure is an error, not an empty trace.
pub fn stdin_trace<R: io::Read>(mut reader: R) -> io::Result<String> {
    let mut buf = String::new();
    reader.read_to_string(&mut buf)?;
    Ok(buf)
}

pub fn execute<W: Write>(query: &QueryContext<'_>, tr: &str, out: &mut W) -> io::Result<()> {
    let text = trace_text(query, tr)?;
    let val = graph_tools::call_from_trace(query.ingest, query.interner, &text);
    crate::serialize::serialize_from_trace_xml(&val, out)
}

report_command! {
    name: "from-trace",
    args: FromTraceArgs,
    static: FROM_TRACE,
    tools: [mcp_tool!(
        "from_trace",
        "Paste a stack trace / compiler error and get it mapped onto indexed symbols innermost-first",
        &[("trace", "string", "Stack trace text")],
        &["trace"],
        |s, a| {
            let val = graph_tools::call_from_trace(s.ingest, s.interner, a.req_str("trace")?);
            render_xml(|out| crate::serialize::serialize_from_trace_xml(&val, out))
        }
    )],
    run: query(execute),
    /// Map a stack trace, sanitizer report or compiler error onto indexed symbols
    #[arg(long = "from-trace", value_name = "FILE")]
    from_trace: Option<String>,
}
