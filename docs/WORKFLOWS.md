# Real-World Workflows: When, Where, & Why to Use ripwire-rs

`ripwire-rs` provides static analysis, call graph navigation, and pre-commit verification gates. Rather than running commands in isolation, `ripwire-rs` is designed around 7 high-impact development and agentic workflows.

---

## 1. Orientation & Scoping (Landing Cold in a Repository)

### When & Where
- First time inspecting or onboarding into an unfamiliar codebase.
- Starting a new issue or feature request.
- Feeding high-level, budget-bounded context to an AI coding agent without blowing prompt token limits.

### Core Operations

- **Global Architecture Map**: Retrieve the top 200 globally central symbols ranked by Personalized PageRank to understand the project backbone.
  ```bash
  ripwire-rs . --top-k=200
  ```

- **File-by-File Inventory**: Compact structural tree displaying the top 3 symbols per file under a strict token budget.
  ```bash
  ripwire-rs . --tree
  ```

- **Goal-Directed Task Scoping**: Retrieve declarations and signatures semantically and structurally relevant to a prompt or goal.
  ```bash
  ripwire-rs . --for="implement user authentication"
  ```

- **Interactive Visual Graph**: Export an in-browser, self-contained SVG/HTML call graph colorized by Leiden community.
  ```bash
  ripwire-rs . --html=graph.html --color-by=community
  ```

- **Graph Export**: Write the resolved call graph, with per-edge provenance, for external graph tools.
  ```bash
  ripwire-rs . --export=graph.graphml   # Gephi, yEd, Neo4j APOC
  ripwire-rs . --export=graph.json      # node-link JSON for NetworkX/D3
  ```

### Benefits
- **Zero Hallucinated Dependencies**: Produces an in-context AST schema dictionary teaching agents the meaning of minified attributes (`t`, `n`, `sc`, `k`, `c`).
- **Prompt Token Savings**: Emits `shown="K"` truncation markers so LLMs know absence in a top-K slice does not mean absence in the codebase.
- **Sub-Second Speed**: Renders complete repository topologies across 36 languages in ~25 ms.

---

## 2. Navigation & Blast Radius (Safe Refactoring & Impact Analysis)

### When & Where
- Before editing or renaming a shared function or method signature.
- During incident post-mortems and bug reproduction to trace caller paths.
- Verifying the blast radius of modifying a core data model or interface.
- Auditing database migration impact before altering SQL schemas or renaming tables.

### Core Operations

- **1-Hop Callers & Outgoing Callees**: Query direct incoming callers or outgoing dependencies for a specific symbol.
  ```bash
  ripwire-rs . --callers="Interner::len"
  ripwire-rs . --callees="ingest_files"
  ```

- **Transitive Blast Radius**: Trace full transitive reachability across downstream callers (up to 16 hops deep) ranked by PageRank.
  ```bash
  ripwire-rs . --impact="Interner"
  ```

- **Local Ego-Graph Neighborhood**: Extract the local call graph around a symbol with custom depth and fanout limits.
  ```bash
  ripwire-rs . --around="Interner::new" --around-depth=2 --around-fanout=16
  ```

- **Shortest Paths & Connecting Subgraphs**: Find the shortest directed call chain between two functions, or compute the minimal connecting subgraph spanning up to 16 symbols.
  ```bash
  ripwire-rs . --path-between="main:execute"
  ripwire-rs . --connect="main,resolve_symbol,emit_xml"
  ```

- **Cross-Language Embedded SQL Table Lineage**: Zero-flag table dependency tracing across application code and SQL schemas.
  ```bash
  ripwire-rs . --callers="user_accounts"
  ripwire-rs . --uses="user_accounts"
  ripwire-rs . --impact="user_accounts"
  ```

- **Intra-Procedural Dataflow Slicing (ARISE)**: Trace variable def-use chains, mutation sites, and lifetimes inside a function body.
  ```bash
  ripwire-rs . --slice="compute:x" --slice-flow=both
  ```

### Benefits
- **Full Transitive Impact**: Identifies downstream callers up to 16 hops deep ranked by Personalized PageRank.
- **Root-Cause Tracing**: `--path-between` and `--connect` expose the exact sequence of intermediary function calls bridging two components.
- **Side-Effect Verification**: ARISE slicing isolates exact variable lifetimes and mutation points inside a function.
- **Cross-Language Database Lineage**: Embedded SQL queries come for free during standard AST ingestion. Any string literal in Dart, Rust, Python, TypeScript, Go, Java, etc. containing SQL DML automatically maps to `.sql` table symbols with typed `read` and `write` roles.
- **Edge Provenance**: `--callees` rows carry `prov="scip"` (pinned by a `--scip=index.scip` overlay), `prov="binding"` (a Python call reaching a C++ definition exposed through pybind11 `.def("name", &fn)`), `prov="import"` (bound through an imported module alias), `prov="final-segment"` (a type qualifier matched by last name), or `prov="split"` (one arm of an ambiguous call); rows bound by name alone (a same-directory or repository-wide match) carry `via="name"`, and rows the caller's own file or enclosing type bound carry neither. Macro callees carry `role="macro"`. With several matched definitions, `--callers` lists `<def ix=>` groups and tags rows `of_def="K"`, `K~s` (split arm), `K~n` (bound by name alone), or `K~t` (primary template chosen over specializations). Declarations tied to a definition share its group; untied declarations carry `bodyless="1"`. Call sites the resolver declined appear as `<dc>` rows with `declined_calls=K`, never as edges.
- **Top-Level Callers**: Calls in a file's module scope (top-level statements and anonymous callback bodies) are attributed to a per-file `<file-scope>` symbol (`t="modscope"`, `p=FILE:1`), so script entrypoints show up in `--callers`, `--impact`, `--uses`, `--edit-check`, and `--affected`. It is a caller, never a callee, with no body to expand.

---

## 3. Pre-Commit & PR Safety Gates (Zero Broken Contracts)

### When & Where
- In local Git pre-commit hooks (`.git/hooks/pre-commit`).
- Before running `git push` or opening a pull request.
- In automated CI/CD PR verification workflows.

### Core Operations

- **Caller Contract Compatibility**: Verify that modified signatures, parameter counts, or visibility did not break 1-hop callers before committing.
  ```bash
  ripwire-rs . --edit-check="symbol_name"
  ripwire-rs . --safe-delete="Interner::len"
  ```

- **Situational Awareness & Targeted Tests**: Map git diff to exact reaching tests and uncover historically coupled co-change files.
  ```bash
  ripwire-rs . --situ
  ripwire-rs . --affected="src/lib.rs"
  ```

- **Untested Blast Radius Gate**: Enforce that changes touch only test-covered code paths (exits code 4 on violation).
  ```bash
  ripwire-rs . --test-gate
  ```

- **Quality Regression Delta Gate**: Prevent regressions in cognitive complexity, nesting depth, or AST clones vs base branch (exits non-zero on regressions).
  ```bash
  ripwire-rs . --quality-delta
  ```

- **Diff Re-Ranking & Integration Seams**: Re-rank the architecture map toward dirty files, or discover untested cross-module integration seams.
  ```bash
  ripwire-rs . --map-diff --top-k=30
  ripwire-rs . --seams
  ```

- **PR Review Bundle & Merge Conflict Scouting**: Generate a composite PR review context bundle vs base branch, or predict semantic conflicts between divergent branches.
  ```bash
  ripwire-rs . --pr-context=main
  ripwire-rs . --merge-scout="branch-a,branch-b"
  ```

### Benefits
- **Surgical Test Execution**: Identifies the exact test files that transitively reach your changes, letting you run a 5-second test subset instead of a 20-minute test suite.
- **Contract Enforcement**: Detects parameter count additions, removals, and visibility changes that break existing callers before you commit.
- **Fowler's Shotgun Surgery Protection**: `--situ` surfaces companion files historically edited together with your diff that you may have forgotten to touch.
- **Proactive Conflict Detection**: `--merge-scout` warns of semantic conflicts across branches before rebasing.

---

## 4. Code Health & Technical Debt Auditing

### When & Where
- Sprint refactoring cycles and technical debt burn-downs.
- Architectural reviews and modularity health checks.
- Eliminating dead weight and duplicate logic.

### Core Operations

- **Dead Code Elimination**: Identify internal functions across the codebase with zero incoming callers.
  ```bash
  ripwire-rs . --dead-code
  ```

- **Maintenance Risk Hotspots**: Rank files where high git churn intersects high cognitive complexity, or sort functions by size.
  ```bash
  ripwire-rs . --hotspots
  ripwire-rs . --biggest-first
  ```

- **AST Code Clones & Linter**: Detect exact and near-miss duplicate code blocks (Type 1-3) and scan for syntax/complexity smells.
  ```bash
  ripwire-rs . --clones --min-tokens=40
  ripwire-rs . --lint
  ripwire-rs . --lint-catalog
  ```

- **Modularity & Layering Rules**: Partition the call graph into Leiden communities, inspect hierarchical clusters, or enforce architectural boundaries.
  ```bash
  ripwire-rs . --communities
  ripwire-rs . --zoom
  ripwire-rs . --arch=rules.txt
  ripwire-rs . --mermaid
  ```

- **Coupling & State Access**: Audit hidden git co-change coupling, inspect external package dependencies, and detect mutable global/static state access.
  ```bash
  ripwire-rs . --cochange
  ripwire-rs . --external-surface
  ripwire-rs . --nonlocal-state
  ```

### Benefits
- **Data-Driven Refactoring**: Prioritizes refactoring on code hotspots where maintenance pain and churn intersect with high complexity.
- **Dead-Weight Removal**: Pinpoints zero-caller functions safely without manual grep sweeps.
- **Architectural Clarity**: Enforces strict layering boundaries via `--arch` and visualizes modular topologies with `--mermaid`.

---

## 5. Deep Inspection & Agent Context Delivery

### When & Where
- Extracting exact function implementations for LLM prompts.
- Debugging stack traces and compiler error messages.
- High-efficiency agent tooling over the Model Context Protocol (MCP).

### Core Operations

- **Symbol Implementation Bodies**: Extract full symbol bodies and callee signatures in CDATA XML blocks, or skeletonize control flow.
  ```bash
  ripwire-rs . --expand="Symbol::new"
  ripwire-rs . --outline="Interner::intern"
  ripwire-rs . --pack-top-n=5
  ripwire-rs . --at="src/main.rs:25"
  ```

- **Verification, Queries & Batching**: Prove relationship claims, query call-graphs with DSL, map compiler traces, or run up to 16 heterogeneous queries in a single turn.
  ```bash
  ripwire-rs . --verify="calls(main,run_cli)"
  ripwire-rs . --graph-query="callers(main)"
  ripwire-rs . --from-trace=error.log
  ripwire-rs . --batch=queries.txt
  ripwire-rs . --callers="Interner::len" --json
  ```

- **Diagnostics, Notes & Agent Protocols**: Run engine health checks, persist symbol notes across sessions, run the stdio MCP server, or install agent skills.
  ```bash
  ripwire-rs . --doctor
  ripwire-rs . --notes
  ripwire-rs . --note-add="symbol: gotcha note"
  ripwire-rs mcp
  ripwire-rs --add-skills
  ```

### Benefits
- **Precision Context Bundles**: Extracts only relevant symbol bodies into CDATA blocks, conserving prompt token budgets.
- **Batching Economy**: `--batch` executes up to 16 queries in a single turn, eliminating agent tool-call round-trip latency.
- **Instant Agent Skills**: `--add-skills` automatically provisions 17 standardized agent skills and active manifest into `~/.agents/skills`.
- **Autonomous Governance**: To make AI coding agents execute these workflows autonomously via pre-commit verification loops, see the [AGENTS.md Integration Guide](AGENTS_GUIDE.md).

---

## 6. Continuous Integration (CI) Workflows & Automated PR Gates

### When & Where
- Automated GitHub Actions (`.github/workflows/`), GitLab CI, or Jenkins pipelines.
- Enforcing pull request quality standards and preventing regressions before merge.

### Core PR Verification Gates

The 4 automated gates designed to run in continuous integration pipelines:

- **PR Situational Awareness**: Emits canonical `ripwire-rs.situ/v1` XML with blast radius, reaching tests, and co-change files; fence it in the PR review UI.
  ```bash
  ripwire-rs . --situ
  ```

- **Architectural Boundary Gate**: Enforces module layering rules from `rules.txt` (exits code 2 on violation).
  ```bash
  ripwire-rs . --arch=rules.txt
  ```

- **Test Reachability Gate**: Verifies that modified paths have test coverage; blocks merge if changes leave an untested blast radius (exits code 4).
  ```bash
  ripwire-rs . --test-gate
  ```

- **Quality Delta Regression Gate**: Blocks PR merge if cognitive complexity, nesting depth, or AST clones regress vs base branch (exits non-zero).
  ```bash
  ripwire-rs . --quality-delta
  ```

### What is `.github/workflows/ripwire-rs-gate.yml`?

The `ripwire-rs-gate.yml` file is a continuous integration (CI) workflow that automatically validates every Pull Request against deterministic static analysis gates before merging into `main` or `master`. It provides 1:1 remote parity with the local pre-commit checks enforced in `AGENTS.md`.

### When to Use It

- **Governing AI Coding Agents**: When AI coding agents generate code and open PRs, this gate blocks untested code paths, broken caller contracts, or complexity spikes automatically.
- **Preventing Architectural Rot**: When teams want automated enforcement of module boundaries (`rules.txt`), zero quality regressions (`--quality-delta`), and test coverage reachability (`--test-gate`).
- **Automated PR Review Summaries**: When you want blast radius, affected test files, and historical co-change files rendered directly into GitHub's `$GITHUB_STEP_SUMMARY` review UI on every push.

### How to Create & Configure It

1. **Create the file**: Add `.github/workflows/ripwire-rs-gate.yml` to your repository.
2. **Install `ripwire-rs`**: Install the CLI in your CI runner via `cargo install ripwire-rs` or `cargo binstall ripwire-rs`.
3. **Adapt for your language stack**: In Step 5 of the workflow below, replace `cargo test --all` with your project's test runner (`npm test`, `pytest`, `go test ./...`, etc.).

### Production GitHub Actions Workflow (`.github/workflows/ripwire-rs-gate.yml`)
```yaml
name: ripwire-rs PR Safety Gates
on:
  pull_request:
    branches: [main, master]

jobs:
  ripwire-rs-gate:
    name: Architecture, Quality & Safety Gates
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4

      - name: Install ripwire-rs
        run: cargo install ripwire-rs

      # 1. Post Situational Awareness directly to GitHub PR Summary
      - name: Generate PR Situational Awareness Report
        run: |
          echo "### ripwire-rs Situational Awareness" >> $GITHUB_STEP_SUMMARY
          echo '```' >> $GITHUB_STEP_SUMMARY
          ripwire-rs . --situ >> $GITHUB_STEP_SUMMARY
          echo '```' >> $GITHUB_STEP_SUMMARY

      # 2. Architectural Boundary Gate (if rules.txt exists)
      - name: Enforce Architecture Layering Rules
        run: |
          if [ -f rules.txt ]; then
            ripwire-rs . --arch=rules.txt
          fi

      # 3. Test Reachability Gate (exits code 4 on untested blast radius)
      - name: Verify Test Reachability
        run: ripwire-rs . --test-gate

      # 4. Quality Delta Regression Gate (exits non-zero on regressions vs base branch)
      - name: Verify Zero Quality Regressions
        run: ripwire-rs . --quality-delta

      # 5. Run Test Suite
      - name: Run Tests
        run: cargo test --all
```

### Contributor Benchmarks & Parity Testing (Optional)

For contributors developing `ripwire-rs` itself, the repository provides Makefile targets to benchmark performance and verify upstream parity:

```bash
# Verify upstream DOM and contract parity (also: make parity or make validate)
make validate-parity REPO=.

# Benchmark execution speed and peak memory against upstream C++ ripwire
make bench REPO=.

# Run statistical micro-benchmarks across in-memory indexing pipelines
make bench-pipeline
```

Parity validation is quiet on success. Failures emit GitHub Actions
`::error::` annotations on stderr and exit nonzero; successful checks are not
listed. This is the default for all three parity targets and the
`scripts/validate-parity.sh` launcher. Make still displays build output.

`make complexity` also emits GitHub Actions `::error file=...,line=...::`
annotations on stderr, one for each Rust function above cognitive complexity
15, and exits nonzero. It is quiet when the gate passes. The
`scripts/check-complexity.py` adapter converts `cccc` JSON output without
capping the number of reported violations.

### Benefits
- **Automated Merge Blocking**: Fails PR jobs automatically (exit code 4 on `--test-gate` or exit code 2 on `--quality-delta`) before untested changes or architectural rot merge into production branches.
- **Microsecond Gate Execution**: Executes static analysis and graph traversals in milliseconds, eliminating slow CI runner bottlenecks.
- **Zero Configuration**: Reads native git repository history, remotes, and branch merge bases automatically.

---

## 7. Multi-Agent Fleet Orchestration (Parallel Worktrees & Subagent Slicing)

### When & Where
- Fanning a multi-symbol feature or large refactoring out across $N$ parallel coding agents or isolated git worktrees.
- Preventing duplicate orientation token spend where every spawned subagent re-reads the same files from scratch.
- Predicting symbol-level merge conflicts, same-file risks, and contract-touch dependencies *before* writing code or before landing concurrent branches.
- Handing off in-flight work and institutional gotcha notes across agent sessions.

### Core Operations

- **Leiden Community Task Slicing**: Carve a goal into 1 shared core plus $N$ disjoint subagent bundles (N=2..16) along call-graph communities.
  ```bash
  ripwire-rs . --pack-task="refactor serialization" --partition=4
  ```

- **Pre-Hoc Lane Conflict Planning**: Predict pairwise symbol collisions, same-file risks, and contract-touch dependencies across worktree lanes before writing code.
  ```bash
  ripwire-rs . --plan-lanes=3 --task="refactor serialization"
  ripwire-rs . --plan-lanes --brief=lanes.txt
  ```

- **Cross-Branch Merge Scouting**: Scout semantic collisions and compute optimal fewest-conflicts-first landing order across concurrent branches.
  ```bash
  ripwire-rs . --merge-scout="feat/auth,feat/billing"
  ```

- **Epistemic Intent Routing & Verification**: Recommend the optimal ripwire-rs verb based on repository evidence, or prove 6 closed relation shapes.
  ```bash
  ripwire-rs . --help-task="plan the new feature for authentication"
  ripwire-rs . --verify="calls(main,run_cli)"
  ```

- **Session Handoff & Symbol Notes**: Package verified changes and co-change partners for subsequent agents, or persist symbol-level gotchas. Co-change partners are mined from the 18-month git window of the first changed file (`score` = co-changes / commits touching that file); `blast_files` counts files outside the changed set that transitively call changed symbols. Branch, subject, and `at` appear only inside a git repository.
  ```bash
  ripwire-rs . --handoff
  ripwire-rs . --note-add="Interner: Spur IDs are 1-based; never index directly without resolve()"
  ripwire-rs . --notes
  ```

### Benefits
- **Zero Redundant Orientation**: `--pack-task --partition=N` carves the task surface along Leiden call-graph communities (with file-level fallback for edgeless data types and median splitting when $K < N$), emitting a `<ctx-partitions>` bundle with a shared core plus $N$ disjoint subagent `<bundle>` slices and Jaccard overlap telemetry (`overlap_mean`, `overlap_max`, `core_overlap`).
- **Pre-Hoc Collision Prevention**: `--plan-lanes` keys claims on path-qualified `fnv1a64(path\0scope\0name)` identities to separate true symbol `conflicts`, `same_file_risk`, and `contract_touch` (a lane's claim sitting inside another lane's transitive caller blast radius), computing a greedy fewest-conflicts-first `landing_order`. Per-lane `conflicts` and `contract_touches` are sums of those pair results; `ambiguous_calls` sums call-graph ambiguity over the lane's claims; `churn` and `hotspot_rank` come from the 12-month git window and are omitted outside a git repository.
- **Epistemic Honesty**: `--help-task` inspects live repository facts (`git`, `dirty`, `trace`, `resolved_symbols`) and abstains when evidence is thin; `--verify` distinguishes `confirmed`, `refuted` (only under complete evidence), and `not-established` (when bounded by the static call-graph or reference floor).
- **Fleet Governance Contract**: For drop-in configuration of autonomous subagents with deterministic static analysis gates, see the [AGENTS.md Integration Guide](AGENTS_GUIDE.md).
