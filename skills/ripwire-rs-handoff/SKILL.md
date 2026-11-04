---
name: ripwire-rs-handoff
description: Brief someone else on a subsystem or an interrupted session: entry points with bodies, design rationale, hotspot risk, pinned notes, and the commit each number was measured at.
allowed-tools: Bash, Read
---

<identity>
You write concise, verified handoff briefs for a successor agent or teammate. Facts only, each stamped with the commit it came from.
</identity>

# Handoff with ripwire

`<dir>` = repo root. `SUBSYSTEM` = the area in plain words. To understand the subsystem yourself, use ripwire-rs-orient instead.

## Subsystem brief

1. `ripwire-rs <dir> --for="SUBSYSTEM" --signatures-only`: the top 10 are what the recipient most needs. Note `cx=` and `in=`.
2. `ripwire-rs <dir> --expand=SYM1,SYM2,SYM3 --legend=compact` for the top three. Paste bodies verbatim. Bodies are redacted by default; `--no-redact` only when auditing credential handling.
3. `ripwire-rs <dir> --recall="SUBSYSTEM"`: summarize the design decisions the docs record. Docs are text, not verified fact; say "the docs claim".
4. `ripwire-rs <dir> --notes --legend=compact`: fold this subsystem's pinned gotchas into the brief. Before handing off, add any trap you found with `--note-add="SYM_or_path: text"`.
5. Risk: `ripwire-rs <subdir> --hotspots` scopes to the subsystem; compare against the repo-wide `--hotspots`. Frame it as "developers keep touching this; tread carefully", not "this has bugs".

Flag any `amb="K"` symbol in `--expand`'s calls as uncertain.

## Interrupted session

`ripwire-rs <dir> --handoff --legend=compact` emits a continuation packet: a `<verified>` section (branch, HEAD sha with `+dirty`, changed files and symbols, blast radius, tests to run) and a `<heuristic>` section (co-change partners, notes, plan pointers) labelled as suggestion. Single root only. Paste it as-is; `--token-budget=N` drops heuristic rows first and reports `withheld=`.

## Stamp every number

A brief is read later, against a moved HEAD. Quote the commit each number was measured at.

| verb | stamp |
|---|---|
| `--quality-delta`, `--pr-context`, `--test-gate`, `--map-diff`, `--doc-drift` | `at="<sha>[+dirty]"` |
| `--stray-content` | `head="<sha>"` (no `+dirty`) |
| `--situ`, `--cochange`, `--owners` | none: run `git rev-parse --short HEAD` yourself |

`+dirty` numbers cannot be reproduced by anyone else: commit, re-measure, or say so in the brief. Traps: `--stray-content` uses `head=`, not `at=`; in `--doc-drift` the header `at=` is a sha but each row's `at=` is a file path.

<output_format>
### Subsystem Handoff Brief (at="<sha>")
- **Core Purpose & Entry Points**: [2 sentences; top symbols with file:line and signatures]
- **Design Rationale**: [decisions from --recall, marked as doc claims]
- **Maintenance Risks**: [hotspots, bus-factor alerts, pinned notes]
- **Continuation Actions**: [next concrete steps]
</output_format>
