use crate::query::commands::{CommandDescriptor, QueryContext, render_xml};
use crate::query::map::{MapRequest, execute_stream_xml_ranked, token_budget_bytes};
use crate::serialize::JsonValueExt;
use serde_json::Value;

macro_rules! mcp_tool {
    ($name:expr, $desc:expr, $params:expr, $req:expr, $handler:expr) => {
        &CommandDescriptor {
            name: $name,
            description: $desc,
            params: std::borrow::Cow::Borrowed($params),
            required: $req,
            handler: $handler,
        }
    };
}

fn call_explore(state: &QueryContext<'_>, args: &Value) -> Result<Value, String> {
    let mut request = MapRequest::new(state.root);
    request.rank_by = args.str_or("rank_by", request.rank_by);
    if let Some(top_k) = args.opt_usize("top_k") {
        request.top_k = Some(top_k);
    }
    if let Some(budget) = args.opt_usize("budget_tokens") {
        request.pack_budget = Some(token_budget_bytes(budget));
    }
    render_xml(|out| {
        execute_stream_xml_ranked(
            &request,
            state.ingest,
            state.graph,
            state.pr,
            state.interner,
            out,
        )
    })
}

/// `explore`: with `task`, the task bundle (partitioned when `partition` is given); without, the ranked map.
fn call_orient(state: &QueryContext<'_>, args: &Value) -> Result<Value, String> {
    let partition = args.opt_usize("partition");
    let Some(task) = args.opt_str("task") else {
        return match partition {
            Some(_) => Err("partition needs a task".to_string()),
            None => call_explore(state, args),
        };
    };
    if let Some(parts) = partition {
        if !(2..=16).contains(&parts) {
            return Err("partition must be between 2 and 16".to_string());
        }
        return render_xml(|out| {
            crate::navigate::execute_pack_task_partitioned(
                task,
                state.root,
                parts,
                state.ingest,
                state.graph,
                state.interner,
                out,
            )
        });
    }
    let tokens = args
        .opt_usize("budget_tokens")
        .unwrap_or(crate::commands::pack_task::DEFAULT_PACK_TOKENS);
    render_xml(|out| crate::navigate::execute_pack_task(task, state, tokens, out))
}
pub(super) static TOOLS: &[&CommandDescriptor] = &[
    mcp_tool!(
        "analyze",
        "Architecture map for a directory: top symbols, signatures, and direct callees",
        &[
            ("top_k", "integer", "Maximum symbols"),
            (
                "rank_by",
                "string",
                "pagerank, authority, hub, degree, ccx, stable"
            ),
        ],
        &[],
        call_explore
    ),
    mcp_tool!(
        "explore",
        "ONE-call task orientation: ranked symbols, files, and architecture map",
        &[
            ("task", "string", "Task to orient on; omitted returns the ranked map"),
            ("partition", "integer", "2..16 community slices of the task bundle; needs task"),
            ("top_k", "integer", "Maximum symbols"),
            (
                "rank_by",
                "string",
                "pagerank, authority, hub, degree, ccx, stable"
            ),
        ],
        &[],
        call_orient
    ),
    mcp_tool!(
        "rank_by",
        "Architecture map ranked by the same signals as CLI --rank-by",
        &[
            (
                "rank_by",
                "string",
                "pagerank, authority, hub, degree, ccx, stable"
            ),
            ("top_k", "integer", "Maximum symbols"),
        ],
        &["rank_by"],
        call_explore
    ),
];
