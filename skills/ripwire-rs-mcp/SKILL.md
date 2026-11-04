---
name: ripwire-rs-mcp
description: Configure ripwire-rs as a stdio Model Context Protocol server, choose between MCP verbs and CLI, and diagnose setup with ripwire-rs --doctor.
allowed-tools: Bash, Read
---

<identity>
You wire ripwire-rs into MCP-native coding agents and pick the right server verb for the question.
</identity>

# ripwire-rs as an MCP server

Auditing someone else's `.mcp.json` is ripwire-rs-security-scan. Agents that can run shell and load skills do not need MCP: the CLI is cheaper in tool-schema overhead.

## Wiring

```json
{
  "mcpServers": {
    "ripwire-rs": { "command": "ripwire-rs", "args": ["mcp"] }
  }
}
```

`ripwire-rs mcp [PATH]` serves the repo at PATH (default `.`). `--scip=FILE` pins call sites to compiler-backed resolution; an unreadable index aborts startup rather than falling back. Verify the setup with `ripwire-rs <repo> --doctor`: binary PATH agreement, grammars, cache, git.

On `initialize` the server returns instructions that clients add to the agent's system prompt. Use `callers`, `callees` and `impact` before grep, and pin same-named definitions with `Owner::method`, a `file` argument, or `"symbol": "src/app.rs:42"`.

## Roots and warm state

Every tool accepts `root`: the repository or worktree for that call. Omit it for the launch root. After `/clear` or a worktree switch, pass the new `root` on every call; the launch root does not follow the session. The server keeps two workspaces warm, so alternating between two roots does not re-index. `path` scopes `for` and the edit verbs; a path outside the loaded root is refused.

## Which verb, and when the CLI is better

Verbs mirror CLI flags: `for` (`--for`), `callers`, `callees`, `impact`, `uses`, `grep`, `defs`, `connect`, `path_between`, `exemplar`, `quality_delta`, `situational_awareness` (`--situ`), `from_trace`, `edit_check`, `fetch_body` (`--expand`), `slice`, `lego`, `batch` (up to 16 reads), and others. Look up the exact argument shape in the tool schema; do not guess it.

Use MCP when you will ask more than once in a session: the parsed index stays in memory, so every answer after the first is warm. Use the CLI for reports and visualizations MCP does not expose, such as `--report`, `--quality-panel`, `--hotspots`, `--clones`, `--arch`, `--seams`, `--test-gate`, `--lint`, `--communities`.

Navigation returns lean reports. Fetch a body only when you will reason about its logic. A `@FILE:LINE` selector resolves to the innermost definition at that line.

## Edit verbs

`replace_symbol_body`, `insert_before_symbol` and `insert_after_symbol` splice text at a symbol's byte span. The receipt carries `edit_check` and `tests_to_run`; do not re-run them. A failed edit leaves the file unchanged. Prefer the CLI (`--replace-symbol-body=SYM --edit-payload=FILE`) unless the server is already warm.

## Editors: `--lsp`

`ripwire-rs --lsp` is a read-only navigation LSP server for editors (definition, references, document symbols, workspace symbols, hover), on the same warm index. Saved-state answers only; counts are floors. It cannot combine with `--mcp` or `--listen`.

<output_format>
### MCP Configuration
- **Transport Mode**: [stdio JSON-RPC 2.0]
- **Configuration Stanza**: [exact JSON snippet]
- **Health Status**: [--doctor result]
</output_format>
