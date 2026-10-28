# CLI Reference

`ripwire-rs` provides both workflow-categorized subcommands and backward-compatible top-level flags.

## Basic Usage

```bash
# Generate ranked symbol call graph map (default 64 KB budget)
ripwire-rs .

# Select ranking signal (pagerank, authority, hub, degree, ccx, stable)
ripwire-rs . --rank-by=authority

# Budget flags are shaping ceilings, not hard gates: output is shaped to fit and never exits non-zero for size.
# --pack-task and --recall report over_budget / over_ceiling only when the minimal unit alone cannot fit.
# Ceilings: --pack-task est_tokens = bytes/4; --recall admits 5*max/2 bytes (est = bytes*2/5); --for windows
# ranked symbols by its token budget (default 4000 tokens); --report defaults to 16 KiB shared across lists.
ripwire-rs . --pack-budget=32768
ripwire-rs . --token-budget=8192
ripwire-rs . --max-tokens=4096
ripwire-rs . --top-k=200

# Control schema legend comment posture (compact or none) and formatting (--pretty, --json)
ripwire-rs . --legend=compact --pretty
ripwire-rs . --top-k=20 --json

# Maps default to 200 symbols and 65536 bytes; unknown rank signals are errors.
# Query symbol relationships by bare name, scoped name, file:name, file::Scope::name or file:LINE
ripwire-rs . --callers="Interner::len"
ripwire-rs . --callees="src/domain.rs:new"
ripwire-rs . --callers="src/domain.rs:42"
ripwire-rs . --impact="Type.method"
# Optional traversal cap (also global with navigate --impact)
ripwire-rs . --impact="Type.method" --impact-depth=3

# Query static use-sites (call, read, write, import, extends)
ripwire-rs . --uses="Interner"

# Cross-language embedded SQL table lineage (zero flags required)
ripwire-rs . --callers="user_accounts"
ripwire-rs . --uses="user_accounts"
ripwire-rs . --impact="user_accounts"
# Export self-contained interactive SVG/HTML visualizer
ripwire-rs . --html=callgraph.html --color-by=community
# Export the call graph for Gephi, yEd, Neo4j (.graphml) or NetworkX/D3 (.json node-link)
ripwire-rs . --export=graph.graphml
```

`--uses` lists all indexed sites by default; use `--limit=N --offset=M` and
`next_offset` to page them. `--impact` keeps unbound candidate calls in `<dc>`
rows outside the graph; `dc_shown` of `declined_calls` says how many a capped sample holds, and `--token-budget` widens it.
`--whereis` scans distinct Git blobs up to 2 MiB once, including unindexed HEAD
files, and replaces changed paths with worktree rows. Outside a git repository it scans the working tree only and reports `git="0"`; it makes no HEAD claim, so `on-head`, `ref`, `at` and `complete` are omitted.
`--lint` compares all distinct naming subjects within each namespace through
512 names. Larger pools compare the next 24 lexicographic successors and mark
the affected rule `count_capped="1"`; paginating findings does not expand that pool.

---
## Language-Agnostic Embedded SQL Lineage

**When & Where:** When auditing database schema changes, tracking table mutations, refactoring SQL migrations, or understanding which backend services and endpoints read from or write to a database table.

SQL queries come **for free automatically** during standard AST ingestion—no new flag is required.

Whenever `ripwire-rs` indexes a repository, any string literal in any host language (Dart, Rust, Python, TypeScript, Go, Java, C#, etc.) containing SQL DML (`SELECT ... FROM`, `JOIN`, `INSERT INTO`, `UPDATE`, `DELETE FROM`) automatically emits `RefRole::Read` and `RefRole::Write` references attributed to the enclosing function or method, and resolves them directly to `.sql` table/view symbols in the call graph.

```bash
# List all functions/methods across the codebase that query or mutate user_accounts
ripwire-rs . --callers="user_accounts"

# List every read and write site with exact file:line and enclosing symbol
ripwire-rs . --uses="user_accounts"

# Calculate transitive blast radius from a table to all downstream callers and tests
ripwire-rs . --impact="user_accounts"

# Safety gates automatically trace from .sql migration files through to application tests
ripwire-rs . --test-gate
ripwire-rs . --situ
```
* **Benefits:** Eliminates the blind spot between database schemas and application code, turning string-embedded SQL queries into typed graph edges without runtime dependencies or database connections.

## Detail Ladder & Progressive Expansion

**When & Where:** When drilling down from architectural map summaries to implementation details of specific functions, methods, or critical hot paths.
```bash
# Expand full source body of symbols and callee signatures
ripwire-rs . --expand="Symbol::new"

# Outline control-flow skeletons of symbols (declarations, loops, branches)
ripwire-rs . --outline="Interner::intern"

# Pack source bodies of the top N highest PageRank symbols
ripwire-rs . --pack-top-n=5

# Orient file by file with top 3 symbols per file
ripwire-rs . --tree
```
* **Benefits:** Delivers exact function bodies with zero boilerplate, eliding irrelevant lines while preserving full type signatures and callee interfaces.

---
## Pre-Commit Verification & Safety Gates

**When & Where:** Local git pre-commit hooks, pre-push checks, or automated CI/CD pull request verification.
```bash
# Verify caller compatibility after editing a symbol contract
ripwire-rs . --edit-check="symbol_name"

# Transitive reachability mapping to affected test suites
ripwire-rs . --affected="src/lib.rs"

# Name tests to run and untested blast radius (exit 4 if non-empty obligations)
ripwire-rs . --test-gate

# Canonical ripwire-rs.situ/v1 XML: blast radius, tests, co-change
ripwire-rs . --situ

# Safe delete assessment joining callers, blast radius, tests, and history
ripwire-rs . --safe-delete="Interner::len"

# Show non-test symbols reached by a test suite
ripwire-rs . --exercises="tests/test_cli_and_mcp.rs"

# Snapshot code quality baseline at current commit (refuses on a dirty tree that already gates)
ripwire-rs . --quality-baseline

# Pin anyway; the absorbed gating count is stamped and reported as baseline_absorbed=
ripwire-rs . --quality-baseline --allow-dirty

# Check for quality regressions against baseline or git HEAD (exits 2 on failure)
ripwire-rs . --quality-delta

# Quality delta of everything since the merge base of main, ignoring any pinned sidecar
ripwire-rs . --quality-delta=main..HEAD

# Print stale ack rows that a regression-free delta collapses to stale_by_kind=
ripwire-rs . --quality-delta --show-stale

# Re-rank map toward git-changed files so recent work floats up
ripwire-rs . --map-diff --top-k=30

# Untested cross-module call boundaries (integration seams)
ripwire-rs . --seams

# Composite PR review context bundle vs base ref
ripwire-rs . --pr-context=main

# Symbol overlap of branch refs vs HEAD: same-symbol conflicts, risks, landing order
ripwire-rs . --merge-scout="branch-a,branch-b"

# Delta Maintainability Model (DMM) scalar in [0,1] measuring healthy code churn
ripwire-rs . --dmm

# Pre-hoc parallel branch collision forecasting before writing code
ripwire-rs . --plan-lanes=3 --task="refactor parser"

# Security audit of agent skill files for exfiltration or command injection
ripwire-rs . --scan-skills

# Pin architectural baseline sidecar or update it with new violations
ripwire-rs . --baseline
ripwire-rs . --baseline-update

# Pin quality census snapshot
ripwire-rs . --pin-census

# Partition quality-delta by author/team ownership scope
ripwire-rs . --quality-delta --scope="core"

# Cluster co-changing files into multi-file change groups and set recurrence threshold
ripwire-rs . --cochange --cochange-groups --cochange-recur=3

# Partition codebase into cohesive modules
ripwire-rs . --partition=5

# Execute transactional versioned JSON edit plan (with --dry-run validation)
ripwire-rs . --dry-run --edit-plan=plan.json
ripwire-rs . --edit-target-file="src/domain.rs" --no-post-check --allow-remote-edits
* **Benefits:** Catches breaking signature changes, eliminates test suite bloat by running only affected tests, and stops architectural decay before merging.

---
## Architecture & Code Intelligence

**When & Where:** Sprint refactoring, architectural reviews, or technical debt auditing.
```bash
# Detect internal functions with zero incoming callers across the tree
ripwire-rs . --dead-code

# Partition call graph into cohesive modules (Louvain community detection)
ripwire-rs . --communities

# Extract ego-graph around a symbol (depth 1, fanout 32)
ripwire-rs . --around="Interner::new" --around-depth=2 --around-fanout=16

# Discover symbols referenced but never defined (stdlib & external dependencies; markdown links, shell builtins and calls through stored fields are not surface)
ripwire-rs . --external-surface

# File-to-file dependency graph with instability and afferent coupling
ripwire-rs . --deps

# Detect AST code clones and near-miss duplicates (Type-1, Type-2, Type-3)
ripwire-rs . --clones --min-tokens=40

# Mine historical git co-change coupling (Fowler's Shotgun Surgery)
ripwire-rs . --cochange

# Compute maintenance hotspots (churn * cognitive complexity)
ripwire-rs . --hotspots

# Enforce architectural layering rules from text file (exits 2 on violation)
ripwire-rs . --arch=rules.txt

# Export Mermaid flowchart of module call dependencies
ripwire-rs . --mermaid

# Multi-level Louvain module hierarchy (computed levels=, top 2 printed)
ripwire-rs . --zoom

# Print every hierarchy level
ripwire-rs . --zoom --zoom-levels=0

# Rank functions by readability volume and size, largest first
ripwire-rs . --biggest-first

# Detect functions accessing reachable global / file-static mutable state (constants count only when mutated)
ripwire-rs . --nonlocal-state

# Scan codebase for AST code smells (XML or SARIF 2.1.0 JSON output)
ripwire-rs . --lint
ripwire-rs . --lint --sarif
# Print built-in AST code smell catalog
ripwire-rs . --lint-catalog

# Compute ARISE variable def-use dataflow slice (forward/backward)
ripwire-rs . --slice="compute:x" --slice-flow=fwd

# Annotate every symbol with fan-in, fan-out, complexity, and change-amplification
ripwire-rs . --metrics

# Entire quality panel in one report across 6 evidence families (strict|default|lenient).
# historical = git commits per file in the 12-month window at or above hcut, the 90th percentile of
# changed files clamped to 3..14 (hbasis= says which bound applied; hstatus=measured-empty means churn was
# read and nothing crossed it); state = mutable non-local state only (configuration reads are not state,
# state_lens=mutable); lexical = 1-2 letter or over-long names; each ranked row lists one <e> child per
# fired family with its measured values.
# Families that cannot be measured (confusion: non-C; historical: no git, shallow) are listed in
# unavailable= and never substituted.
ripwire-rs . --quality-panel=default

# Show struct memory layout with computed LP64 field offsets and alignment padding
ripwire-rs . --layout="Symbol"

# Cache-conscious struct field affinity analysis against 64-byte lines
ripwire-rs . --field-affinity="Symbol"

# Local-reasoning lens measuring share of context outside a unit's file
ripwire-rs . --context-ratio

# Four orthogonal evidence families joined, ranked by distinct families fired
ripwire-rs . --ensemble

# Doc comment to identifier coherence verification
ripwire-rs . --comment-coherence

# Set maximum hop depth for intra-procedural slicing (default: 8)
ripwire-rs . --slice="compute:x" --slice-flow=fwd --slice-depth=12

# Select or ignore specific AST lint rules
ripwire-rs . --lint --lint-select="large-function,deep-nesting"
ripwire-rs . --lint --lint-ignore="magic-number"

# Calibrate naming rules against git rename history
ripwire-rs . --naming-consistency --naming-calibration

# Generate executive markdown architecture report (16 KiB by default, shared by five lists;
# each list shows "(showing S of T)"; page with --limit/--offset, follow a capped list's next route)
ripwire-rs . --report
ripwire-rs . --report --limit=12 --offset=12

# Emit full schema legend dictionary
ripwire-rs . --legend-dict

# Emit deterministic path-ordered map without volatile rank scores
ripwire-rs . --stable

# Disable stable dependency violation section in --deps
ripwire-rs . --deps --no-stable
* **Benefits:** Focuses engineering attention directly on defect-prone hotspots, hidden file coupling, and dead code with mathematical precision.

---
## Task Retrieval & Batching

**When & Where:** Debugging compiler errors/stack traces, precision prompting for AI agents, or multi-query workflows.
```bash
# Find best-in-class existing implementation of kind to imitate
ripwire-rs . --exemplar="parser"

# Show one interface contract and all types implementing it
ripwire-rs . --lego="ToXmlString"

# Search repository markdown documentation for relevant plans and notes
ripwire-rs . --recall="PageRank"

# Map stack trace / compiler error lines onto indexed symbols
ripwire-rs . --from-trace=error.log

# Emit JSON instead of XML for navigation queries
ripwire-rs . --callers="Interner::len" --json

# Execute up to 16 heterogeneous sub-queries in a single turn
ripwire-rs . --batch=queries.txt
# Lines: name:argument or name:{JSON arguments}; blank/comment lines ignored.
# More than 16 queries errors; recursive batch/writes error per item.
# --json preserves report hierarchy, text/CDATA, and interleaved mixed content.
# Root tag/attributes/items remain; leaf text uses "text". Reserved-name attributes
# use "attributes"; namespaced names use {uri}local. Malformed reports error.

# Resolve innermost symbol chain enclosing FILE:LINE
ripwire-rs . --at="src/main.rs:25"

# Formally verify relationship claim across 6 closed shapes (calls, uses, unused, contains, defines, reaches)
ripwire-rs . --verify="calls(main,run_cli)"

# Single-call orientation task bundle under budget (or partitioned across N parallel subagents)
ripwire-rs . --pack-task="search"
ripwire-rs . --pack-task="search" --partition=4

# Pre-hoc parallel worktree lane planner (auto-carved by community or from explicit --brief=FILE)
ripwire-rs . --plan-lanes=3 --task="refactor serialization"
ripwire-rs . --plan-lanes --brief=lanes.txt

# Reach of one feature flag: the symbols reading it, their transitive callers, and affected tests.
# GATE must name a flag exactly as --flags lists it; an empty or unknown gate exits nonzero.
ripwire-rs . --flip=FEATURE_X
# Functional graph query DSL
ripwire-rs . --graph-query="callers(main)"

# Self-check diagnostics
ripwire-rs . --doctor

# Pinned session gotchas and notes
ripwire-rs . --notes
ripwire-rs . --note-add="symbol: gotcha note"

# Conceptual queries (several words) also match each symbol's own source text, admit a body that calls or
# mentions two or more query concepts, and rank bodyless declarations and interface contracts below
# implementations. A one-word exact symbol name keeps name-only scoring.

# Importance-weighted detail: full bodies for top N symbols, signatures for rest
ripwire-rs . --for="parser" --detail=5

# Cut returned set at the relevance cliff (Adaptive-k)
ripwire-rs . --for="parser" --adaptive

# Print declaration skeletons with bodies elided (~80% token savings)
ripwire-rs . --for="parser" --pack-signatures

# Restore the interface/implementor <lego> section collapsed to a counted stub by default
ripwire-rs . --for="parser" --sections=lego
# Find which local branch defines or mentions a symbol (HEAD first, with git history oracle)
ripwire-rs . --whereis="Symbol" --with-history --whereis-full-text --whereis-listing=defs

# Sweep local branches for divergent authored work vs merge-base with HEAD
ripwire-rs . --stray-content

# Self-evaluations: co-change recall and known-item retrieval benchmarks
ripwire-rs . --eval
ripwire-rs . --eval-retrieval
# Score name-based call resolution against a compiler SCIP index (rust-analyzer scip, scip-typescript, scip-python)
ripwire-rs . --eval-scip=index.scip
# Every disagreement, for triage (default 10 per class and reason). unindexed= counts edges into
# files the index holds no document for; they are sampled but left out of precision.
ripwire-rs . --eval-scip=index.scip --eval-samples=100000
# Install 17 pre-packaged ripwire-rs agent skills to ~/.agents/skills (or custom path)
ripwire-rs --add-skills

# Span-tiered literal search: code tier by default, --grep-in=any serves comments and strings too
ripwire-rs . --grep="Interner" --grep-in=any --grep-context=2
# Conjunction and negation terms (repeatable; --grep-scope=file widens them to the whole file), path scope
ripwire-rs . --grep="cache" --and="payload" --not="deprecated" --in="src/cache.rs"

# Emit compact refetch handles and re-expand them
ripwire-rs . --for="parser" --handles
ripwire-rs . --refetch="h:abc123"

# Analyze a remote repository: a shallow clone cached under $TMPDIR/ripwire-rs/, reused across runs
ripwire-rs https://github.com/org/repo --report
ripwire-rs https://github.com/org/repo --refetch --report

# Recommend the best ripwire-rs verb for a natural-language task
ripwire-rs . --help-task="find callers of function and test blast radius"

# Crawler filtering: exclude patterns, ignore gitignore, ignore test files, file size limits
ripwire-rs . --exclude="vendor/*" --exclude=dist,build --no-ignore --ignore-tests --max-file-size=500000

# Compress output with compact token dialect
ripwire-rs . --compress

# Cold-parse once into an index file, then reuse it (valid while HEAD, working tree and binary match)
ripwire-rs . --index-out="index.json"
ripwire-rs . --cache="index.json" --callers="parse"

# Annotate symbols with runtime profile weights or trace execution
ripwire-rs . --with-profile="flamegraph.perf"
ripwire-rs . --run-trace="cargo test" --run-timeout=30

# Additional benchmark evaluation modes: mined git commits, skills, and stray branches
ripwire-rs . --eval-mined
ripwire-rs . --eval-skills
ripwire-rs . --eval-stray
```
* **Benefits:** Minimal prompt token consumption with maximum semantic density for coding agents.

### Indexed variable reads

`--uses=LIMIT` lists actual syntax-backed `role="read"` sites for supported bare
value expressions in Python, JavaScript/TypeScript, and Rust, in addition to the
existing Elixir and SQL read captures. `--grep=LIMIT` uses those same references
for enclosing-row `reads=` / `reads_at=` and the `--uses=LIMIT` continuation.
No corpus text scan estimates value usage.

Declaration names, assignment targets, keyword/property labels, comments and
literal text are not reads. Lexical locals and parameters are excluded from
repository-wide name resolution, including JavaScript block/function bindings
and Rust's source-ordered `let` shadowing. Callable values remain read sites, not
call edges; SQL table lineage retains its existing graph semantics. A TypeScript or JavaScript
`const` bound to a provably scalar initializer (a literal, a template or `+`/`??`/`||` of scalars, or
`process.env.NAME` with optional `!`/`as`) is an immutable binding, not state. A const object or array
stays a read site, and its contents remain state through member writes.

Read coverage is deliberately conservative, not universal across grammars:
unresolved local-variable uses, Python class-body namespace lookup, Rust macro
token trees, and other languages without read captures are not reconstructed
from text. These results remain static evidence floors, not whole-program
binding or alias analysis.

---
## Workflow Subcommands

- `ripwire-rs orient [PATH] [--for <TASK>] [--scip <FILE>]`: Codebase orientation and task-ranked summaries.
- `ripwire-rs navigate [PATH] --symbol <SYM> [--callers|--callees|--impact] [--scip <FILE>]`: Symbol neighborhood analysis.
- `ripwire-rs gate [PATH] [--edit-check|--affected|--quality-delta|--test-gate]`: Verification safety gates.
- `ripwire-rs metrics [PATH] [--hotspots|--complexity|--clones]`: Code health metrics.
- `ripwire-rs slice [PATH] <SYM:VAR> [--flow <fwd|back|both>]`: Intra-procedural slicing.
- `ripwire-rs mcp [PATH] [--scip <FILE>]`: Launch native JSON-RPC Model Context Protocol (MCP) server over stdio.
- `ripwire-rs [PATH] --lsp`: Launch stdio JSON-RPC Language Server Protocol (LSP) server bridge.
- `ripwire-rs [PATH] [--cache <PATH>] [--no-cache]`: Persistent binary on-disk caching in TMPDIR (or custom path) for sub-second repeat queries. Crawler options (`--exclude`, `--no-ignore`, `--ignore-tests`, `--max-file-size`) are part of the cache identity, so each option set keeps its own cache.
- `ripwire-rs [PATH] --scip=<index.scip>`: Overlay a compiler-backed SCIP index: the call sites it resolves bind exactly (`prov="scip"`, map header `precise=K`) and sites it resolves outside the index count as external; other sites keep name resolution. Bypasses the cache.
  The same `--scip=PATH` overlay works with `navigate`, plain and task-ranked `orient`, and `mcp` (also `[PATH] --mcp --scip=PATH`). MCP retains the loaded overlay for the session. Unreadable or malformed indexes fail explicitly without an unpinned fallback.
  `--scip` is global: it can appear before or after the subcommand. Multiline calls match the callee identifier's line in the index; reported call-site lines stay at the call expression's start.
- `make bench [REPO=<PATH>]`: Multi-metric performance and resident set size (RSS) benchmark against C++ `ripwire`.
- `make bench-pipeline`: Native in-memory Cargo micro-benchmarks (`cargo bench --bench bench_pipeline`).
