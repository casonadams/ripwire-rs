//! Batch lookups: `--batch FILE` runs newline-delimited verb:arg lines in one turn.
use super::{Command, SharedOption};
use super::common::mcp::mcp_tool;
use crate::cli::Cli;
use crate::commands::context::{OutputDialect, load_repo_analysis_for_cli};
use crate::domain::{IngestResult, Interner};
use crate::graph::Graph;
use crate::query::commands::{CommandDescriptor, QueryContext, execute_batch_item};
use crate::rank::PageRankResult;
use clap::{Args, Command as ClapCommand};
use serde_json::{Value, json};
use std::io::{self, Write};

/// Top-level batch flag.
#[derive(Args, Debug, Clone, Default, PartialEq)]
pub struct BatchFlags {
    /// Run several lookups in one turn from newline-delimited verb:arg file
    #[arg(long, value_name = "FILE")]
    pub batch: Option<String>,
}

pub struct Batch;
pub static BATCH: Batch = Batch;

impl Command for Batch {
    fn name(&self) -> &'static str {
        "batch"
    }

    fn options(&self) -> &'static [SharedOption] {
        &[]
    }

    fn augment(&self, cmd: ClapCommand) -> ClapCommand {
        BatchFlags::augment_args(cmd)
    }

    fn run(&self, cli: &Cli, root: &str) -> Option<io::Result<()>> {
        let file = cli.plugins.decode::<BatchFlags>()?.batch?;
        Some((|| {
            let (ingest, graph, pr, interner) = load_repo_analysis_for_cli(cli, root, false)?;
            let mut stdout_lock = io::stdout().lock();
            let mut stdout = cli.wrap_xml_writer(&mut stdout_lock, false);
            execute_batch_file(root, &file, &ingest, &graph, &pr, &interner, &mut stdout)?;
            stdout.finish()
        })())
    }

    fn tools(&self) -> &'static [&'static CommandDescriptor] {
        TOOLS
    }
}

static TOOLS: &[&CommandDescriptor] = &[mcp_tool!(
    "batch",
    "ONE-TURN CONTEXT SWEEP: answer up to 16 heterogeneous READ sub-queries in a single call",
    &[(
        "queries",
        "array",
        r#"Up to 16 tool calls, each {"name": "callers", "arguments": {"symbol": "run"}} or {"tool": "callers", "symbol": "run"}"#,
    )],
    &["queries"],
    call_batch
)];

fn call_batch(state: &QueryContext<'_>, args: &Value) -> Result<Value, String> {
    let queries = args["queries"]
        .as_array()
        .ok_or("queries must be an array")?;
    if queries.len() > crate::query::commands::BATCH_LIMIT {
        return Err(format!(
            "batch supports at most {} queries",
            crate::query::commands::BATCH_LIMIT
        ));
    }
    Ok(Value::Array(
        queries
            .iter()
            .map(|query| match batch_item(query) {
                Ok((name, arguments)) => {
                    execute_batch_item(state, crate::commands::tools(), name, &arguments)
                }
                Err(error) => json!({"tool": query.get("name").or_else(|| query.get("tool")), "error": error}),
            })
            .collect(),
    ))
}

/// Agents tend to send the flat `{"tool": T, ...}` shape, so both shapes are accepted.
fn batch_item(query: &Value) -> Result<(&str, Value), String> {
    const SHAPE: &str =
        r#"expected {"name": TOOL, "arguments": {...}} or {"tool": TOOL, ...arguments}"#;
    let object = query.as_object().ok_or(format!("batch item is not an object; {SHAPE}"))?;
    let name = ["name", "tool"]
        .iter()
        .find_map(|key| object.get(*key).and_then(Value::as_str))
        .ok_or(format!("batch item names no tool; {SHAPE}"))?;
    let rest = object
        .iter()
        .filter(|(key, _)| !matches!(key.as_str(), "name" | "tool" | "arguments"));
    match object.get("arguments") {
        Some(arguments) if rest.clone().next().is_none() => Ok((name, arguments.clone())),
        Some(_) => Err(format!("batch item mixes \"arguments\" with other fields; {SHAPE}")),
        None => Ok((name, Value::Object(rest.map(|(k, v)| (k.clone(), v.clone())).collect()))),
    }
}

pub fn execute_batch_content<W: Write>(
    root: &str,
    content: &str,
    ingest: &IngestResult,
    graph: &Graph,
    pr: &PageRankResult,
    interner: &Interner,
    out: &mut W,
) -> io::Result<()> {
    use crate::query::commands::{QueryContext, execute_batch, parse_batch_content};
    let sources = crate::query::FileSources {
        root: std::path::Path::new(root),
    };
    let at = crate::git::history::compute_git_at_stamp(root);
    let context = QueryContext {
        root,
        ingest,
        graph,
        pr,
        interner,
        sources: &sources,
        commits: None,
        at: at.as_deref(),
    };
    let tools = crate::commands::tools();
    let queries = parse_batch_content(tools, content);
    let results = execute_batch(&context, tools, &queries).map_err(io::Error::other)?;
    write_batch_results(root, &results, out)
}

pub fn write_batch_results<W: Write>(
    root: &str,
    results: &[serde_json::Value],
    out: &mut W,
) -> io::Result<()> {
    use crate::serialize::XmlEscapeExt;
    writeln!(
        out,
        "<batch schema=\"ripwire-rs.batch/v1\" root=\"{}\" dictv=\"{}\" count=\"{}\">",
        root.xml_escaped(),
        crate::serialize::LEGEND_DICT_VERSION,
        results.len(),
    )?;
    for result in results {
        let name = result["tool"].as_str().unwrap_or("");
        writeln!(out, "  <result query=\"{}\">", name.xml_escaped())?;
        if let Some(error) = result.get("error").and_then(serde_json::Value::as_str) {
            writeln!(out, "<error>{}</error>", error.xml_escaped())?;
        } else if result["kind"] == "xml" {
            let value = result["result"]
                .as_str()
                .ok_or_else(|| io::Error::other("XML batch result must be a string"))?;
            out.write_all(value.as_bytes())?;
            writeln!(out)?;
        } else if let Some(value) = result["result"].as_str() {
            writeln!(out, "{}", value.xml_escaped())?;
        } else {
            let value = serde_json::to_string(&result["result"]).map_err(io::Error::other)?;
            writeln!(out, "{}", value.xml_escaped())?;
        }
        writeln!(out, "  </result>")?;
    }
    writeln!(out, "</batch>")
}
pub fn execute_batch_file<W: Write>(
    root: &str,
    file_path: &str,
    ingest: &IngestResult,
    graph: &Graph,
    pr: &PageRankResult,
    interner: &Interner,
    out: &mut W,
) -> io::Result<()> {
    let content = if file_path == "-" {
        crate::commands::common::edit::read_stdin_text()?
    } else {
        std::fs::read_to_string(file_path)?
    };
    execute_batch_content(root, &content, ingest, graph, pr, interner, out)
}
