---
name: ripwire-rs-orient
description: Entry point for ripwire-rs: orient on an unfamiliar repo, route the current moment to the right skill, and map before reading whole files.
allowed-tools: Bash, Read
---

<identity>
You orient a coding agent cold-starting on a repository, with the fewest tokens, and send it to the one skill that fits its moment.
</identity>

# Orient with ripwire

`ripwire-rs` is on PATH. `<dir>` = repo root or the subsystem you work in. The first call on a tree parses; later calls are warm, so chain freely. Several roots (`ripwire-rs dir1 dir2 --report`) give one merged map for a split checkout.

## Orient once, then look up lean

1. `ripwire-rs <dir> --recall="<task>"` - full bodies of relevant design docs. If a doc answers the question, stop.
2. `ripwire-rs <dir> --report` - modules, god-files (ranked by `afferent`), cycles, top PageRank symbols.
3. `ripwire-rs <dir> --for="<task in words>"` - ranked signatures with `cx=` and `in=`. A name-exact query (`--for=buildGraph`) gets exact matching; a phrase gets subtoken+body ranking.
4. `ripwire-rs <dir> --hotspots --legend=compact` - churn x complexity. Read these files first.

Only the plain `ripwire-rs <dir>` map carries `<inv>` (unshown files and entry points). Read it before concluding a file is absent. `--expand`, `--callers` and other lookups need no map.

Budget: `--top-k=50` or `--token-budget=8000` shape the map. On `--for`, bound output with `--signatures-only` or `--token-budget=N`. A thin answer (`coverage=` under 50, or `weak="1"`) means widen: `--for="<task>" --limit=40`, then `--offset` for the next page. For fan-out, `--pack-task="<task>" --partition=N`.

## Route the moment

| Moment | Skill |
|---|---|
| Who calls X, what X calls, a body, a literal, a flow | ripwire-rs-navigate |
| Symptom, stack trace, compiler error, regression | ripwire-rs-navigate (bug branches) |
| About to write a function, class, feature, or dependency | ripwire-rs-before-you-build |
| Edited a symbol, reviewing a diff, which tests, writing tests | ripwire-rs-change-check |
| Did my change make things worse, unfamiliar risk, layering | ripwire-rs-quality-bar |
| Brief someone else on a subsystem | ripwire-rs-handoff |
| Measured slow path (benchmark, profile) | ripwire-rs-perf-target |
| Untrusted skill or MCP config, security-sensitive code | ripwire-rs-security-scan |
| Run ripwire as an MCP server, or check its health | ripwire-rs-mcp |

Unsure? `ripwire-rs <dir> --help-task="<task in words>"` names one verb, or abstains.

## Honesty signals

Rows marked `via="name"` are name-based hedges, not proven edges. `declined`, `unresolved`, `amb="K"` (K ambiguous calls), `counts_floor` and `capped="1"` name what was left out. Read the source at that site before a decision depends on it. Files outside the index (over `--max-file-size`, or an unsupported language) have no graph edges; `--grep` still searches unindexed text files under the size ceiling. A git root drops `.gitignore`d paths unless `--no-ignore` is passed.

## Notes

`--notes` lists gotchas pinned by earlier sessions. `--note-add="SYM_or_path: what went wrong"` pins one for the next agent.

<output_format>
### Architecture Orientation
- **Subsystem Overview**: [modules and entry points from --report]
- **Task-Relevant Symbols**: [top --for hits]
- **Key God-Files & Hotspots**: [files to inspect first]
- **Recommended Next Step**: [skill and command]
</output_format>
