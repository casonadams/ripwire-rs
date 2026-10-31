---
name: ripwire-rs-router
description: >
  Route to the right ripwire skill for your task. Use when unsure where to start,
  landing on unfamiliar code, refactoring, or pre-commit verification.
allowed-tools: Bash, Read
---

<identity>
You route a coding agent to the one ripwire skill and CLI command that fits its current moment, using the fewest tokens.
</identity>

<instructions>
# ripwire router: moment to skill

ripwire-rs answers two questions with the smallest structural answer: **Find** (where is it, who calls it, what
breaks) and **Check** (what does my change affect, did it get worse). Match the moment, enter ONE skill.
Unsure? `ripwire-rs <dir> --help-task="<task in words>"` names the one command, or honestly abstains.
`ripwire-rs -h` lists only the core flags; `--help` lists all.

| The moment | Skill | Opening move |
|---|---|---|
| Cold start in an unfamiliar repo | **ripwire-rs-orient** | `--for="<task>"` (or the bare map once) |
| Who calls X / what does X call / what breaks if I change X | **ripwire-rs-navigate** | `--callers=X`, `--callees=X`, `--impact=X` |
| A name has several definitions; which one, how do I pin it | **ripwire-rs-navigate** | `--defs=A,B` (rows carry `sel=`) |
| Find an exact literal, error text or config key | **ripwire-rs-navigate** | `--grep='literal'` |
| I have a stack trace, sanitizer report or compiler error | **ripwire-rs-find-bug** | `--from-trace=FILE` |
| Debugging a symptom | **ripwire-rs-find-bug** | `--for=symptom`, `--situ` |
| About to write a function or class | **ripwire-rs-reuse-first** | `--exemplar="<sub-task>"`, `--for="<task>"` |
| Planning a feature or an interface implementation | **ripwire-rs-before-you-build** | `--recall`, `--for`, `--lego=I` |
| Just edited a symbol: did I break a caller | **ripwire-rs-change-check** | `--edit-check=SYM` |
| Reviewing my diff before push | **ripwire-rs-change-check** | `--quality-delta`, then `--situ`, `--test-gate` |
| Which tests should I run | **ripwire-rs-change-check** | `--affected=FILES`, `--test-gate` |
| "Is this done": did I make anything worse | **ripwire-rs-quality-bar** | `--quality-delta` (exit 2 = new gating debt) |
| Reviewing code I did not write, or a refactor target | **ripwire-rs-fresh-eyes** | `--quality-panel`, `--hotspots`, `--communities` |
| Writing tests for untested code | **ripwire-rs-write-tests** | `--seams`, `--callers=SYM` |
| A benchmark or profile names a slow symbol | **ripwire-rs-perf-target** | measure, navigate, re-measure |
| Untrusted input, or auditing a skill or MCP config | **ripwire-rs-security-scan** | `--lint`, `--scan-skills` |
| Writing a handoff summary | **ripwire-rs-handoff** | the handoff bundle |
| Running ripwire as an MCP server | **ripwire-rs-mcp** | `ripwire-rs mcp` |
| Layering or dependency health, CI boundaries | **ripwire-rs-layers** | `--arch`, `--deps` |
| A question no fixed verb phrases | **ripwire-rs-graph-query** | `--graph-query=EXPR` |

## Before you reach for a default

| About to | Use instead |
|---|---|
| `Read` a whole file to understand one function | `--expand=SYM` (body plus callee signatures) |
| `Read` several files to learn how something works | `--pack-task="<task>"` (ranking, bodies, callers, tests under one budget) |
| `grep` a symbol name | `--for="exactName"` (name-exact, includes callers) or `--uses=SYM` |
| `grep` a concept | `--for="<the concept in words>"` |
| paste a stack trace and pick frames by hand | `--from-trace=FILE` |
| `git diff` to judge a change | `--situ` |

## Trust what the answer says it does not know

Rows labelled `via="name"` are hedges, not proven edges. `declined`, `unresolved`, `counts_floor` and
`capped="1"` name what was left out. When a decision rides on one of those, read the source at that site.

## Before you call it done

Run `ripwire-rs <dir> --quality-delta` (exit 2 means new gating debt), then `--edit-check=SYM` for a changed
contract and `--affected` for the tests that prove it. Pin a floor first with `--quality-baseline`.

Full map, legend rules, wrong-tool cases and the two-reflex primer: read [`reference.md`](reference.md).
</instructions>

<output_format>
### Routing Decision
- **Moment**: [what the agent is about to do]
- **Skill**: [one skill name]
- **Opening command**: `ripwire-rs <dir> ...`
</output_format>
