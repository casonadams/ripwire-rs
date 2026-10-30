use crate::query::commands::{CommandDescriptor, dispatch};
use crate::workspace::Workspace;
use serde_json::{Value, json};

pub fn dispatch_tool_action(
    state: &Workspace,
    tools: &[&CommandDescriptor],
    tool_name: &str,
    args: &Value,
) -> Result<Value, String> {
    let sources = crate::query::FileSources {
        root: std::path::Path::new(&state.root),
    };
    dispatch(&state.query_context(&sources), tools, tool_name, args)
}

/// The root a call runs against: its `root` argument, else the server's launch root.
fn requested_root(args: &Value, default_root: &str) -> Result<String, String> {
    let Some(root) = args.get("root").and_then(Value::as_str) else {
        return Ok(default_root.to_string());
    };
    if !std::path::Path::new(root).is_dir() {
        return Err(format!("root '{root}' is not a directory"));
    }
    Ok(root.to_string())
}

fn same_root(a: &str, b: &str) -> bool {
    a == b
        || std::fs::canonicalize(a)
            .and_then(|a| std::fs::canonicalize(b).map(|b| a == b))
            .unwrap_or(false)
}

/// Point `state` at the root this call names, loading it when it is not the loaded one. A server pinned by a
/// SCIP overlay serves only its launch root, since the overlay would be lost on a switch.
fn select_workspace(
    state: &mut Option<Workspace>,
    args: &Value,
    default_root: &str,
) -> Result<(), String> {
    let target = requested_root(args, default_root)?;
    match state.as_ref() {
        Some(ws) if same_root(&ws.root, &target) => Ok(()),
        Some(ws) if ws.has_scip() => Err(format!(
            "this server is pinned to '{}' by --scip; a per-call root is unavailable",
            ws.root
        )),
        _ => {
            *state = Workspace::load(&target);
            Ok(())
        }
    }
}

#[must_use]
pub fn handle_tools_call(
    id: &Value,
    params: &Value,
    state: &mut Option<Workspace>,
    default_root: &str,
    tools: &[&CommandDescriptor],
) -> Value {
    let Some(tool_name) = params["name"].as_str() else {
        return json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32602, "message": "missing tool 'name'" }
        });
    };

    let empty_args = json!({});
    let args = params.get("arguments").unwrap_or(&empty_args);
    if let Err(message) = select_workspace(state, args, default_root) {
        return json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32602, "message": message }
        });
    }
    let Some(current_state) = state.as_mut() else {
        return json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32603, "message": "failed to index workspace root" }
        });
    };
    if let Err(err) = current_state.refresh_if_stale() {
        return json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32603, "message": format!("failed to re-index workspace: {err}") }
        });
    }

    match dispatch_tool_action(current_state, tools, tool_name, args) {
        Ok(val) => {
            let text = match val {
                Value::String(s) => s,
                other => other.to_string(),
            };
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "content": [{ "type": "text", "text": text }]
                }
            })
        }
        Err(err_msg) => json!({
            "jsonrpc": "2.0",
            "id": id,
            "error": { "code": -32602, "message": err_msg }
        }),
    }
}
