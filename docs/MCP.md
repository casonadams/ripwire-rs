# Model Context Protocol (MCP)

`ripwire-rs` provides a native stdio Model Context Protocol (MCP) server conforming to JSON-RPC 2.0.


> **Skills vs. MCP**: If your coding agent supports skills (`ripwire-rs --add-skills`) or project instructions ([`AGENTS.md`](AGENTS_GUIDE.md)) with shell execution, **you do not need the MCP server**—the agent invokes the `ripwire-rs` CLI directly with lower tool-schema overhead. Use `ripwire-rs mcp` only for MCP-native clients or environments without direct CLI/skill execution.
## Client Registration

### Standard MCP Configuration (`mcp.json`)

```json
{
  "mcpServers": {
    "ripwire-rs": {
      "command": "/path/to/ripwire-rs",
      "args": ["mcp"]
    }
  }
}
```

On `initialize` the server returns `instructions`, which MCP clients place in the agent's system
prompt. They say when the navigation tools answer better than grep (callers, callees, impact) and
that every call needs `symbol`, pinned to one definition among same-named ones by an
owner-qualified name (`Owner::method` or `Owner.method`), the `file` argument, or a
`file:LINE` location. Navigation tools (`callers`, `find_referencing_symbols`, `callees`,
`impact`, `find_symbol`, `uses`) take `file`; an unknown argument's error lists the accepted ones.

---

## Tools Inventory

| Tool | Category | Description |
| :--- | :--- | :--- |
| `explore` | Retrieval | Ranked map, bodies, callers, and test targets in a single byte-budgeted response. |
| `for` | Retrieval | Task-lens relevance ranking using BM25 and Personalized PageRank. |
| `batch` | Retrieval | Execute up to 16 heterogeneous sub-queries in a single call. |
| `fetch_body` | Retrieval | Fetch symbol definition source code by handle or name with optional line bounds. |
| `exemplar` | Retrieval | Retrieve the best-in-class implementation of a given role (fn, class, interface). |
| `lego` | Retrieval | Interface contract view with all implementors across the workspace. |
| `grep` | Search | Trigram literal search annotated with enclosing symbols and definitions. |
| `find_symbol` | Navigation | 1-hop symbol neighborhood: callers, callees, definitions, and ambiguity metrics. |
| `find_referencing_symbols` | Navigation | Direct (1-hop) callers of a symbol or database table. |
| `callers` | Navigation | Who calls a function or method: each call site's enclosing function, same-named definitions kept apart. |
| `callees` | Navigation | What a function or method calls, with edge provenance and declined evidence. |
| `impact` | Navigation | What breaks if a symbol or database table changes: transitive callers via the call hierarchy. |
| `uses` | Navigation | Statically resolvable use-sites (call, read, write, import, extends), including embedded SQL table lineage from application strings. |
| `path_between` | Architecture | Shortest directed call path between two symbols. |
| `connect` | Architecture | Directed connecting subgraph relating 2..16 symbols, with radius 2 by default. |
| `analyze` | Architecture | Architecture map ranked by PageRank or chosen metric. |
| `rank_by` | Architecture | Architecture map ranked by pagerank, authority, hub, degree, ccx, or stable. |
| `situational_awareness` | Architecture | Diff context: blast radius, tests to run, and forgotten co-change partners. |
| `from_trace` | Debugging | Maps stack trace frames and compiler errors onto indexed symbol definitions. |
| `slice` | Slicing | Intra-procedural def-use variable slicing (forward, backward, or both). |
| `cochange` | Git Analytics | Historical Shotgun Surgery co-change coupling. |
| `owners` | Git Analytics | Recency-weighted author ownership per file. |
| `edit_check` | Safety Gates | Verify caller compatibility after editing a function contract. |
| `affected` | Safety Gates | Transitive test reachability mapping from changed symbols. |
| `quality_baseline` | Safety Gates | Snapshot code quality floor at current commit; refuses on a dirty tree that already gates unless `allow_dirty` is true. |
| `quality_delta` | Safety Gates | Pre-commit quality regression self-check against baseline or git HEAD. |
| `doc_drift` | Documentation | Detect out-of-bounds file:line references in markdown documents. |
| `mentions` | Documentation | Locate markdown design notes referencing a code symbol in backticks. |
| `memory_recall` | Documentation | Relevant documentation and design notes matching a task. |
| `flags` | Inspection | Dark feature flags, `#ifdef` guards, and runtime environment variable reads. |
| `whereis` | Inspection | Branch divergence tracker: finds which branch's tree defines a symbol. |
| `stray_content` | Inspection | Lines authored on a divergent branch not present in HEAD. |
| `replace_symbol_body` | Atomic Edit | Atomically replace an entire symbol definition with verified byte rollback. |
| `insert_before_symbol` | Atomic Edit | Atomically insert source code before a symbol definition. |
| `insert_after_symbol` | Atomic Edit | Atomically insert source code after a symbol definition. |

## Canonical Reports

Overlapping CLI commands and MCP tools use one query implementation and report.
Read the canonical XML/text in JSON-RPC `result.content[0].text`; do not parse
legacy JSON summary fields such as navigation `definitions` or impact arrays.
Facts, defaults, errors, limits, and pagination match the CLI. Navigation retains
definition grouping, edge provenance, and unresolved/external/declined evidence.
Map defaults are 200 symbols and 8192 bytes. Supported rank signals are
`pagerank`, `authority`, `hub`, `degree`, `ccx`, and `stable`; unknown signals
(including `churn`) are errors, not a fallback.
`situational_awareness` and CLI `--situ` emit `ripwire-rs.situ/v1` XML.
Slicing uses the same workspace root, flow/depth validation, and errors as the CLI.

`--scip=FILE` is global (before or after a subcommand), bypasses the analysis
cache, and is retained for the MCP session. Multiline calls match the callee
identifier's index line; report lines remain the call expression's start.

Batch accepts at most 16 read queries; larger requests fail as a whole rather
than truncating. Individual failures remain per-item errors (`{tool,error}` in
MCP, `<error>` in CLI). Recursive batches and writes, including
`quality_baseline` and quality-delta acknowledgment options, are rejected per item.
Each MCP batch item is `{"name": TOOL, "arguments": {...}}`, or the tool's arguments may sit
beside it (`{"tool": "callers", "symbol": "run"}`); an item mixing both is rejected with the
expected shape. CLI batch files ignore blank/comment lines and accept `name:argument` or
`name:{JSON arguments}` for the shared read options.

Impact `max_depth` matches the optional global CLI `--impact-depth=N`, including
`navigate --impact`; the default traversal is unchanged. `find_symbol` retains
grouped definitions and the same bound/declined neighbors as navigation.
Legends are preserved from shared reports, without per-session mutation.
Batch executes against the real root through the same command registry. Success
entries retain `{tool,result,kind}`, with `kind` set to `xml`, `text`, or `json`;
malformed query names fail only their own item. CLI XML embeds XML results and
escapes text/JSON results.
`owners` accepts integer `detail > 0` to unfold file/author facts.
`whereis` accepts `listing: "defs"|"refs"|"all"` and `full_text: true|false`;
invalid listings fail as on the CLI. Reports retain the real workspace `root`
and revision `at`. Detail 2 includes all requested bodies with accurate
`shown`/`total`; connect radius metadata reports the effective radius (default 2).
