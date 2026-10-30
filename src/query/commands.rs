use super::SourceReader;
use crate::domain::{IngestResult, Interner};
use crate::git::history::CommitInfo;
use crate::graph::Graph;
use crate::rank::PageRankResult;
use serde_json::{Value, json};
/// Borrowed facts shared by all query transports.
pub struct QueryContext<'a> {
    pub root: &'a str,
    pub ingest: &'a IngestResult,
    pub graph: &'a Graph,
    pub pr: &'a PageRankResult,
    pub interner: &'a Interner,
    pub commits: Option<&'a [CommitInfo]>,
    pub sources: &'a dyn SourceReader,
    pub at: Option<&'a str>,
}

impl<'a> crate::domain::CorpusView<'a> for &QueryContext<'a> {
    fn ingest(&self) -> &'a IngestResult {
        self.ingest
    }
    fn interner(&self) -> &'a Interner {
        self.interner
    }
}

pub fn render_xml(
    render: impl FnOnce(&mut Vec<u8>) -> std::io::Result<()>,
) -> Result<Value, String> {
    let mut bytes = Vec::new();
    render(&mut bytes).map_err(|error| error.to_string())?;
    String::from_utf8(bytes)
        .map(Value::String)
        .map_err(|error| error.to_string())
}

pub fn render_query_value(
    mut value: Value,
    args: &Value,
    render: impl FnOnce(&Value, &mut Vec<u8>) -> std::io::Result<()>,
) -> Result<Value, String> {
    if let Some(error) = value.get("error").and_then(Value::as_str) {
        return Err(error.to_string());
    }
    for field in ["limit", "offset"] {
        if let Some(number) = args.get(field).and_then(Value::as_u64) {
            value[field] = number.into();
        }
    }
    render_xml(|out| render(&value, out))
}

pub fn resolve_state_commits<'a>(
    state: &QueryContext<'a>,
    months: u32,
) -> std::borrow::Cow<'a, [CommitInfo]> {
    if let Some(commits) = state.commits {
        return std::borrow::Cow::Borrowed(commits);
    }
    let cutoff = crate::git::history::resolve_cutoff_timestamp(None, months);
    std::borrow::Cow::Owned(
        crate::git::history::walk_repository_history_scoped(state.root, cutoff).unwrap_or_default(),
    )
}

/// Transport-neutral command metadata and implementation. `params` are `(name, type, description)` rows; a tool
/// built by the registry owns the rows its command's shared options add.
#[derive(Clone)]
pub struct CommandDescriptor {
    pub name: &'static str,
    pub description: &'static str,
    pub params: std::borrow::Cow<'static, [(&'static str, &'static str, &'static str)]>,
    pub required: &'static [&'static str],
    pub handler: fn(&QueryContext<'_>, &Value) -> Result<Value, String>,
}

pub fn validate_arguments(command: &CommandDescriptor, args: &Value) -> Result<(), String> {
    let object = args.as_object().ok_or("arguments must be an object")?;
    for required in command.required {
        if !object.contains_key(*required) {
            return Err(format!("missing required argument '{required}'"));
        }
    }
    for (name, value) in object {
        let (_, kind, _) = command
            .params
            .iter()
            .find(|(parameter, _, _)| *parameter == name)
            .ok_or_else(|| {
                let accepted: Vec<&str> = command.params.iter().map(|(p, _, _)| *p).collect();
                format!(
                    "unknown argument '{name}' for {}; accepted: {}",
                    command.name,
                    accepted.join(", ")
                )
            })?;
        validate_argument_type(name, kind, value)?;
    }
    Ok(())
}

fn validate_argument_type(name: &str, kind: &str, value: &Value) -> Result<(), String> {
    let valid = match kind {
        "string" => value.is_string(),
        "boolean" => value.is_boolean(),
        "integer" => value
            .as_u64()
            .is_some_and(|number| usize::try_from(number).is_ok()),
        "array" => value.is_array(),
        "string[]" => value
            .as_array()
            .is_some_and(|items| items.iter().all(Value::is_string)),
        _ => false,
    };
    if !valid {
        return Err(format!("argument '{name}' must be {kind}"));
    }
    if matches!(name, "max_depth" | "radius" | "recur")
        && value
            .as_u64()
            .is_some_and(|number| number > u32::MAX as u64)
    {
        return Err(format!("argument '{name}' exceeds the supported depth"));
    }
    Ok(())
}

/// Dispatch a validated command without transport or session state.
pub fn dispatch(
    context: &QueryContext<'_>,
    tools: &[&CommandDescriptor],
    name: &str,
    args: &Value,
) -> Result<Value, String> {
    let command = tools
        .iter()
        .copied()
        .find(|command| command.name == name)
        .ok_or_else(|| format!("unknown query: {name}"))?;
    validate_arguments(command, args)?;
    let legend = legend_posture(args)?;
    check_envelope_scope(command, context.root, args)?;
    let seeded = resolve_seeds(command, context, args)?;
    let args = scoped_args(command, &seeded);
    let result = (command.handler)(context, &args)?;
    Ok(match (legend, result) {
        (Legend::None, Value::String(text)) => Value::String(strip_legend_comment(text)),
        (_, result) => result,
    })
}

/// Envelope parameters every MCP tool accepts unless it declares the name itself (`path` filters grep, owners and
/// the edit verbs). `path` and `paths` are client scope hints: `paths` is accepted and not acted on, and `path`
/// only scopes tools that take `in_scope` (see [`scoped_args`]). `root` selects the workspace the MCP server
/// indexes for that call (see `mcp::dispatch`). `legend` is honored by [`dispatch`].
pub const ENVELOPE_ROWS: [(&str, &str, &str); 4] = [
    (
        "path",
        "string",
        "Client path hint: a subdirectory scopes tools that take in_scope",
    ),
    ("paths", "string[]", "Client workspace path hints"),
    (
        "legend",
        "string",
        "Schema legend posture: compact (default) or none",
    ),
    (
        "root",
        "string",
        "Directory of the repository or worktree to query for this call; default is the server's launch root",
    ),
];

const SEED_PARAMS: [&str; 4] = ["symbol", "from", "to", "interface"];

fn is_seed(value: &Value) -> bool {
    value.as_str().is_some_and(|s| s.starts_with('@'))
}

/// Check each `@FILE:LINE` selector resolves, so a bad one is refused with its cause. The resolver binds the
/// definition itself; `uses` takes the bare name. The name-matching scans (`owners`, `mentions`, `whereis`) refuse a
/// seed and say which name to retry.
fn resolve_seeds<'a>(
    command: &CommandDescriptor,
    context: &QueryContext<'_>,
    args: &'a Value,
) -> Result<std::borrow::Cow<'a, Value>, String> {
    let listed = args.get("symbols").and_then(Value::as_array);
    let has_seed = SEED_PARAMS
        .iter()
        .any(|key| args.get(key).is_some_and(is_seed))
        || listed.is_some_and(|items| items.iter().any(is_seed));
    if !has_seed {
        return Ok(std::borrow::Cow::Borrowed(args));
    }
    let resolve = |value: &mut Value| -> Result<(), String> {
        let Some(spec) = value.as_str().filter(|s| s.starts_with('@')) else {
            return Ok(());
        };
        let seed = crate::navigate::resolve_seed(spec, context.ingest, context.interner)?;
        match command.name {
            "owners" | "mentions" | "whereis" => {
                return Err(format!(
                    "{} matches by name and does not take a seed; '{spec}' is definition '{}'",
                    command.name, seed.name
                ));
            }
            "uses" => *value = Value::from(seed.name),
            _ => {}
        }
        Ok(())
    };
    let mut resolved = args.clone();
    for key in SEED_PARAMS {
        if let Some(value) = resolved.get_mut(key) {
            resolve(value)?;
        }
    }
    if let Some(Value::Array(items)) = resolved.get_mut("symbols") {
        items.iter_mut().try_for_each(resolve)?;
    }
    Ok(std::borrow::Cow::Owned(resolved))
}

/// Refuse an envelope `path` or `paths` entry that is an absolute directory outside the loaded root: it asks for
/// another repository, which only `root` can select, and ignoring it would answer from the wrong one.
fn check_envelope_scope(
    command: &CommandDescriptor,
    root: &str,
    args: &Value,
) -> Result<(), String> {
    let envelope_path = command.params.iter().any(|row| *row == ENVELOPE_ROWS[0]);
    let own_path = args.get("path").filter(|_| envelope_path);
    let listed = args.get("paths").and_then(Value::as_array);
    for entry in own_path.into_iter().chain(listed.into_iter().flatten()) {
        let Some(path) = entry.as_str().map(std::path::Path::new) else {
            continue;
        };
        if path.is_absolute() && !path_within(path, root) {
            return Err(format!(
                "'{}' is outside the loaded root '{root}'; select a repository with the root argument",
                path.display()
            ));
        }
    }
    Ok(())
}

fn path_within(path: &std::path::Path, root: &str) -> bool {
    let canon = |p: &std::path::Path| std::fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    path.starts_with(root) || canon(path).starts_with(canon(std::path::Path::new(root)))
}

/// `args` with the envelope `path` subdirectory carried into `in_scope` for a tool that takes `in_scope` and
/// does not declare its own `path`.
fn scoped_args<'a>(command: &CommandDescriptor, args: &'a Value) -> std::borrow::Cow<'a, Value> {
    let takes_scope = command
        .params
        .iter()
        .any(|(name, _, _)| *name == "in_scope");
    let envelope_path = command.params.iter().any(|row| *row == ENVELOPE_ROWS[0]);
    let sub = args
        .get("path")
        .and_then(Value::as_str)
        .map(|path| path.trim_start_matches("./").trim_end_matches('/'))
        .filter(|path| !path.is_empty() && *path != ".");
    match sub {
        Some(sub) if takes_scope && envelope_path && args.get("in_scope").is_none() => {
            let mut scoped = args.clone();
            scoped["in_scope"] = Value::from(sub);
            std::borrow::Cow::Owned(scoped)
        }
        _ => std::borrow::Cow::Borrowed(args),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Legend {
    Compact,
    None,
}

fn legend_posture(args: &Value) -> Result<Legend, String> {
    match args.get("legend").and_then(Value::as_str) {
        None | Some("compact") => Ok(Legend::Compact),
        Some("none") => Ok(Legend::None),
        Some(other) => Err(format!(
            "argument 'legend' must be compact or none, got '{other}'"
        )),
    }
}

/// Drop the leading `<!-- ... -->` schema legend line a report opens with.
fn strip_legend_comment(text: String) -> String {
    match text.split_once('\n') {
        Some((first, rest)) if first.starts_with("<!--") && first.ends_with("-->") => {
            rest.to_string()
        }
        _ => text,
    }
}

pub const BATCH_LIMIT: usize = 16;

#[derive(serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum BatchResultKind {
    Xml,
    Text,
    Json,
}

pub struct BatchQuery {
    pub name: String,
    pub arguments: Value,
}

fn batch_read_only(name: &str, arguments: &Value) -> bool {
    !matches!(
        name,
        "batch"
            | "quality_baseline"
            | "insert_after_symbol"
            | "insert_before_symbol"
            | "replace_symbol_body"
    ) && !(name == "quality_delta"
        && (arguments.get("quality_ack").is_some() || arguments.get("ack_only").is_some()))
}

pub fn execute_batch(
    context: &QueryContext<'_>,
    tools: &[&CommandDescriptor],
    queries: &[BatchQuery],
) -> Result<Vec<Value>, String> {
    if queries.len() > BATCH_LIMIT {
        return Err(format!("batch supports at most {BATCH_LIMIT} queries"));
    }
    Ok(queries
        .iter()
        .map(|query| execute_batch_item(context, tools, &query.name, &query.arguments))
        .collect())
}

pub fn execute_batch_item(
    context: &QueryContext<'_>,
    tools: &[&CommandDescriptor],
    name: &str,
    arguments: &Value,
) -> Value {
    let result = if arguments.get("root").is_some() {
        Err("root applies to the whole batch: pass it on the batch call".to_string())
    } else if batch_read_only(name, arguments) {
        dispatch(context, tools, name, arguments)
    } else {
        Err(format!(
            "batch permits only non-recursive read queries: {name}"
        ))
    };
    match result {
        Ok(value) => {
            let kind = if !value.is_string() {
                BatchResultKind::Json
            } else if name == "memory_recall" {
                BatchResultKind::Text
            } else {
                BatchResultKind::Xml
            };
            json!({"tool": name, "result": value, "kind": kind})
        }
        Err(error) => json!({"tool": name, "error": error}),
    }
}

/// The argument a bare `name: text` batch line fills: the command's first required string parameter,
/// else its first declared string parameter when it has no required parameters, else `symbol`.
fn batch_key(tools: &[&CommandDescriptor], name: &str) -> &'static str {
    let Some(command) = tools.iter().copied().find(|command| command.name == name) else {
        return "symbol";
    };
    let is_string = |param: &str| {
        command
            .params
            .iter()
            .any(|(declared, kind, _)| *declared == param && *kind == "string")
    };
    if command.required.is_empty() {
        command
            .params
            .iter()
            .find(|(_, kind, _)| *kind == "string")
            .map_or("symbol", |(param, _, _)| *param)
    } else {
        command
            .required
            .iter()
            .copied()
            .find(|required| is_string(required))
            .unwrap_or("symbol")
    }
}

/// Parse the CLI line format, also accepting a JSON argument object after `name:`.
pub fn parse_batch_content(tools: &[&CommandDescriptor], content: &str) -> Vec<BatchQuery> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .take(BATCH_LIMIT + 1)
        .map(|line| {
            let (name, text) = line.split_once(':').unwrap_or((line, ""));
            let name = name.trim();
            let text = text.trim();
            let arguments = if text.starts_with('{') {
                serde_json::from_str(text).unwrap_or_else(|_| Value::String(text.to_string()))
            } else {
                let key = batch_key(tools, name);
                if text.is_empty() {
                    json!({})
                } else {
                    json!({ (key): text })
                }
            };
            BatchQuery {
                name: name.to_string(),
                arguments,
            }
        })
        .collect()
}
