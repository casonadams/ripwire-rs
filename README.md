# ripwire-rs

High-throughput agent context router and deterministic static analysis engine.

Rust port and rewrite of [redhat-et/ripwire](https://github.com/redhat-et/ripwire), originally created and authored by David Brewster at Red Hat Emerging Technologies.

`ripwire-rs` analyzes source code across 36 languages to construct precise call graphs, calculate symbol impact via PageRank and HITS, mine co-change patterns, slice intra-procedural dataflow, and enforce pre-commit quality gates. It runs as a standalone CLI or as a Model Context Protocol (MCP) server over stdio for AI coding agents.


> **100% Local & Zero Telemetry**: `ripwire-rs` is an offline-by-design, security-first static analysis engine. It contains **zero network calls, zero telemetry, zero analytics, and zero model training**. Your source code never leaves your machine. It purely extracts local AST context, call graphs, and metrics, piping structured XML/JSON to stdout or stdio MCP for whichever agent harness or workflow engine *you* choose to run. Automated secret redaction is enabled by default.
---

## Design Principles

Built for **maximum intelligence in the smallest token footprint**:

- **Present-Only Serialization**: Omits zero/unit default attributes (`ccx="0"`, `in="0"`, `churn="1"`) and places pagination window metadata (`shown=`, `total=`, `capped=`, `next=`) once on the root element.
- **Noise-Free Ranking**: Filters out non-callable noise, test/fixture helpers, language builtins, and prose backticks before ranking; partitions unresolvable calls into `unresolved`, `external`, and `declined`.
- **Actionable Context in One Turn**: Bundles top implementation bodies, resolved callee signatures (`<calls>`), architectural layer tags (`layer=`), and causal def-use slices (`--slice`) directly into responses.
- **Sub-50ms Zero-Buffer Pipeline**: Interned `Spur`/`NodeId` tables, contiguous CSR graphs, in-process `gix` history walking, and direct `io::Write` XML/JSON streaming across 36 Tree-Sitter languages (with automatic embedded SQL table lineage).

---

## Installation

```bash
# macOS & Linux (Homebrew)
brew install casonadams/tap/ripwire-rs

# Ubuntu & Debian (APT)
echo "deb [trusted=yes] https://casonadams.github.io/homebrew-tap/apt stable main" | sudo tee /etc/apt/sources.list.d/casonadams.list
sudo apt update && sudo apt install ripwire-rs

# Fedora, RHEL & CentOS (DNF / RPM)
curl -fsSL https://casonadams.github.io/homebrew-tap/rpm/casonadams.repo | sudo tee /etc/yum.repos.d/casonadams.repo
sudo dnf install ripwire-rs

# Rust (prebuilt binary via cargo-binstall, or compile from source)
cargo binstall ripwire-rs
cargo install ripwire-rs
```

Install the 17 embedded agent skills with `ripwire-rs --add-skills`—agents with shell and skill support invoke the CLI directly and **do not need an MCP server**. See the **[Installation Guide](docs/INSTALL.md)** for standalone `.deb`/`.rpm` downloads and source builds, or **[MCP Configuration](docs/MCP.md)** if your client requires stdio MCP.

---

## Workflows

See the **[Workflow Guide](docs/WORKFLOWS.md)** (or **[online](https://casonadams.github.io/ripwire-rs/workflows.html)**) for complete scenarios:

1. **[Orientation & Scoping](docs/WORKFLOWS.md#1-orientation--scoping-landing-cold-in-a-repository)**: Cold-start repository maps and task-ranked context (`--top-k`, `--tree`, `--for`, `--html`).
2. **[Navigation & Blast Radius](docs/WORKFLOWS.md#2-navigation--blast-radius-safe-refactoring--impact-analysis)**: Dependency tracing, SQL table lineage, and dataflow slicing (`--callers`, `--callees`, `--impact`, `--uses`, `--connect`, `--slice`).
3. **[Pre-Commit & PR Safety Gates](docs/WORKFLOWS.md#3-pre-commit--pr-safety-gates-zero-broken-contracts)**: Contract checks, test reachability, and diff awareness (`--edit-check`, `--affected`, `--test-gate`, `--situ`, `--pr-context`).
4. **[Code Health & Technical Debt](docs/WORKFLOWS.md#4-code-health--technical-debt-auditing)**: Quality regressions, dead code, hotspots, clones, and coupling (`--quality-delta`, `--quality-panel`, `--dead-code`, `--hotspots`, `--clones`, `--cochange`, `--deps`).
5. **[Deep Inspection & Agent Context](docs/WORKFLOWS.md#5-deep-inspection--agent-context-delivery)**: Token-budgeted symbol bodies, trace mapping, and multi-query batching (`--expand`, `--outline`, `--compress`, `--from-trace`, `--batch`, `--json`).
6. **[CI Gates & Agent Integration](docs/WORKFLOWS.md#6-continuous-integration-ci-workflows--automated-pr-gates)**: Automated PR gates, stdio MCP server, LSP bridge, and 17 agent skills (`--add-skills`, `ripwire-rs mcp`, `--lsp`, `--scan-skills`).
7. **[Multi-Agent Fleet Orchestration](docs/WORKFLOWS.md#7-multi-agent-fleet-orchestration-parallel-worktrees--subagent-slicing)**: Community-carved task partitioning, pre-hoc worktree lane collision planning, cross-branch merge scouting, and session handoffs (`--pack-task --partition=N`, `--plan-lanes`, `--brief`, `--merge-scout`, `--handoff`, `--help-task`, `--verify`).

Compiler-backed resolution is available with `--scip=index.scip` on top-level
queries, `navigate`, `orient` (including `--for`), and `mcp`:
`ripwire-rs navigate . --symbol main --callees --scip=index.scip`.
The overlay bypasses the analysis cache; MCP retains it for the session.
Unreadable or malformed indexes fail explicitly without a name-only fallback.
`--scip` is global (before or after a subcommand). Multiline calls match the
callee identifier's index line; report lines preserve the call expression start.
Overlapping MCP tools return canonical CLI XML/text in
`result.content[0].text`, with the same facts, defaults, errors, and limits.
Maps default to 200 symbols / 8192 bytes; rank signals are `pagerank`,
`authority`, `hub`, `degree`, `ccx`, and `stable` (unknown signals error).
`--situ` emits `ripwire-rs.situ/v1` XML; connecting subgraphs default to radius 2.
`--impact-depth=N` optionally limits impact traversal (MCP `max_depth`), including
`navigate --impact`. CLI `--json` preserves canonical XML hierarchy and text/CDATA;
malformed reports fail instead of producing summary fallbacks.

Navigation reports expose their limits: `--impact` lists unbound candidate call
sites as `<dc>` rows, never as graph edges. `--uses` lists all indexed sites by
default; add `--limit=N --offset=M` to page them. Literal search reports span
tiers and suppressed mentions; `--grep-in=any` includes comments and strings,
and `--in=PATH` restricts paths. `--whereis` scans distinct Git blobs up to 2 MiB
once, including unindexed HEAD files and changed worktree paths.
Python, JavaScript/TypeScript, and Rust also index bare non-local variable reads;
grep's `reads=` comes from those references, not text estimates. Unsupported
binding forms remain evidence floors.

---

## Documentation

- **[Installation Guide](docs/INSTALL.md)** — Homebrew, APT, DNF/RPM, `cargo-binstall`, and source build instructions.
- **[Workflow Guide](docs/WORKFLOWS.md)** — In-depth guide to the 7 primary workflows: when, where, and why to use each command.
- **[AGENTS.md Governance Guide](docs/AGENTS_GUIDE.md)** — Drop-in `AGENTS.md` template, 5-step autonomous verification loop, and CI/CD parity.
- **[CLI Reference](docs/CLI.md)** — Complete command catalog, top-level flags, streaming budgets, and selector syntax. **[Generated flag and MCP tool index](docs/REFERENCE.md)** — every flag and tool, produced from the binary.
- **[Pre-Commit Safety Gates](docs/GATES.md)** — Verification gates (`--edit-check`, `--affected`, `--quality-delta`, `--test-gate`, `--seams`, `--pr-context`, `--merge-scout`, `--arch`).
- **[Model Context Protocol (MCP)](docs/MCP.md)** — Optional stdio MCP server for clients without direct CLI/skill support.
- **[Architecture & Pipeline](docs/ARCHITECTURE.md)** — Tree-Sitter grammars, lock-free interning, CSR in-edge graph, and streaming XML.
- **[Algorithms & Academic Lenses](docs/ALGORITHMS.md)** — PageRank, HITS, Leiden modularity, clone detector, and dataflow slicing.
- **[Interactive Documentation Site](https://casonadams.github.io/ripwire-rs/)** — Showcase, command catalog, and visualizer.

---

## Supported Languages (36)

First-class Tree-Sitter AST queries extract definitions, signatures, imports, and call sites across 36 languages, with automatic embedded SQL table lineage (`SELECT`, `JOIN`, `INSERT`, `UPDATE`, `DELETE`) resolved across host-language string literals.

| Domain | Supported Languages & Extensions |
| :--- | :--- |
| **Systems & Native** | **Rust** (`.rs`), **C** (`.c`, `.h`), **C++** (`.cpp`, `.cc`, `.cxx`, `.hpp`, `.hxx`, `.hh`, `.h`), **Zig** (`.zig`, `.zon`), **Go** (`.go`), **Objective-C / Objective-C++** (`.m`, `.mm`), **Fortran** (`.f`, `.for`, `.f90`, `.f95`, `.f03`, `.f08`), **COBOL** (`.cbl`, `.cob`, `.cpy`) |
| **GPU & Shaders** | **CUDA** (`.cu`, `.cuh`), **Metal** (`.metal`) |
| **Web & Frontend** | **TypeScript** (`.ts`, `.tsx`, `.mts`, `.cts`, `.astro`), **JavaScript** (`.js`, `.jsx`, `.mjs`, `.cjs`) |
| **JVM & Enterprise** | **Java** (`.java`), **Kotlin** (`.kt`, `.kts`), **Scala** (`.scala`, `.sc`), **C#** (`.cs`) |
| **Scripting & Backend** | **Python** (`.py`, `.pyi`), **Ruby** (`.rb`, `.rake`, `.jbuilder`), **PHP** (`.php`, `.phtml`), **Elixir** (`.ex`, `.exs`), **Erlang** (`.erl`, `.hrl`), **Lua** (`.lua`), **Bash / Shell** (`.sh`, `.bash`, `.zsh`) |
| **Mobile, Game & Media** | **Swift** (`.swift`), **Dart** (`.dart`), **GDScript** (`.gd`), **BrightScript** (`.brs`, `.bs`) |
| **Data, Schema & Smart Contracts** | **SQL** (`.sql`, Dataform `.sqlx` models with `ref()` lineage), **GraphQL** (`.graphql`, `.gql`), **Protocol Buffers** (`.proto`), **Solidity** (`.sol`) |
| **Infra, Config & Docs** | **HCL / Terraform** (`.tf`, `.hcl`, `.tfvars`), **YAML** (`.yml`, `.yaml`), **TOML** (`.toml`), **JSON** (`.json`), **Markdown / Docs** (`.md`, `.markdown`, `.mdx`, `.rst`, `.adoc`, `.org`) |

---

## Development

```bash
# Run all quality checks (clippy, complexity <= 15, quality delta, crap <= 30, fmt)
make all

# Run test suite under coverage
make crap

# Format code
make fmt
```

`make all`, `make crap`, and `make coverage` use the nextest `ci` profile
when nextest is installed: passing tests stay quiet, failure output appears
once at the end, and all tests run even after a failure. Doc tests run
separately within `make coverage`. Without nextest, coverage uses cargo test.

Agents can parse `target/nextest/ci/junit.xml` for test names, results,
durations, and captured failure output. GitHub Actions uploads this report
as the `nextest-results` artifact when available. Console output is concise
text, not a machine-readable stream.

Override `NEXTEST_PROFILE` or `NEXTEST_SHOW_PROGRESS` to select different
reporting, for example `NEXTEST_PROFILE=default make coverage` for per-test
status lines.
