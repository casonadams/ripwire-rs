# Architecture

`ripwire-rs` is a deterministic static analysis engine and agent context router implemented in pure Rust.

## Pipeline Overview

```
[Candidate Source Files]
           │
           ▼
┌──────────────────────┐
│ Deterministic Crawl  │  Byte-sorted path ordering, denylist pruning, size bounds
└──────────┬───────────┘
           │
           ▼
┌──────────────────────┐
│ Parallel AST Ingest  │  36 Tree-Sitter grammars, lock-free lasso interning, SoA tables
└──────────┬───────────┘
           │
           ▼
┌──────────────────────┐
│ Reference Resolution │  Lexical precedence ladder: same-file > same-dir > global
└──────────┬───────────┘
           │
           ▼
┌──────────────────────┐
│ CSR In-Edge Graph    │  Transpose Compressed Sparse Row matrix for cache-line locality
└──────────┬───────────┘
           │
           ├────────────────────────┬────────────────────────┐
           ▼                        ▼                        ▼
┌──────────────────────┐ ┌──────────────────────┐ ┌──────────────────────┐
│ PageRank / HITS      │ │ Community Detection  │ │ Academic Lenses      │
│ 1024-block reduction │ │ Louvain modularity   │ │ ARISE, Clones, Churn │
└──────────┬───────────┘ └──────────┬───────────┘ └──────────┬───────────┘
           │                        │                        │
           └────────────────────────┴────────────────────────┘
                                    │
                                    ▼
┌──────────────────────┐
│ Streamed XML Output  │  64 KB BufWriter direct-to-stdout without document accumulation
└──────────────────────┘
```

## Key Architectural Invariants

1. **Deterministic Execution**:
   - File crawling collects all valid source paths and sorts them lexicographically by byte order before assigning IDs.
   - All symbol IDs, graph traversals, and algorithms use deterministic tie-breaking (no random seeds, no hash iteration order leakage).

2. **Memory Efficiency**:
   - Symbols and references reside in Structure-of-Arrays (SoA) vectors addressed by 32-bit `NodeId` handles.
   - String interning backed by concurrent `lasso::ThreadedRodeo` eliminates duplicate string allocations.
   - Graph representations utilize Compressed Sparse Row (CSR) matrices for high compute density and cache locality.

3. **Streaming XML Serialization**:
   - Output serializes directly to a 64 KB `BufWriter` on `stdout`.
   - Never accumulates the full serialized document in memory.
   - Enforces configurable pack budgets (default 64 KB / 16K tokens) to prevent context saturation.

4. **Language-Agnostic Embedded SQL Lineage**:
   - Zero extra flags: executes automatically during standard parallel AST ingest.
   - String literals, raw strings, and template strings across all host languages (Dart, Rust, Python, TypeScript, Go, Java, C#, etc.) are scanned for SQL DML keyword sequences (`SELECT ... FROM`, `JOIN`, `INSERT INTO`, `UPDATE`, `DELETE FROM`).
   - Keyword pairs classify table accesses into typed reference roles (`RefRole::Read` and `RefRole::Write`).
   - The reference resolution ladder binds these table accesses directly to `.sql` DDL definitions (`CREATE TABLE ...`), unifying database entities into the same CSR in-edge graph as application functions and methods.

5. **Budget-Derived Output Caps**:
   - Every default row cap is computed from the output budget (`PageRequest.budget`, default 64 KB, or `--token-budget` / `--max-tokens`) through `Paged` / `PageRequest::window_fit`, never a literal count.
   - An explicit shared `--limit` overrides the computed cap; `--offset` moves the window.
   - A command needs only `Paged::new(rows, run.page(), cost)`, where `cost` is the row's bytes (`row_bytes` for serialized rows, `text_bytes` for plain text parts).

6. **Budget Contract (shaping, not gating)**:
   - A token budget is a ceiling the output is shaped to. Nothing is withheld by a hard gate, and no exit status changes with the budget.
   - `--pack-task`: `est_tokens` is `ceil(bytes / 4)` of the whole document, framing included. Signatures take half the budget, then bodies, then callers and tests, each window measured row by row. A document never exceeds its ceiling unless only the minimal unit (the top-ranked signature) cannot fit; then `over_budget="1"` discloses the overrun.
   - `--recall`: the ceiling is `5 * max_tokens / 2` bytes, matching its estimate `est_tokens = ceil(bytes * 2 / 5)`. Admission shrinks until the rendered answer, headers and footer included, fits. Only when the best-ranked document's minimal excerpt cannot fit is the answer returned over, with `over_ceiling=1`.
   - `--for`: the ranked symbols are windowed by the token budget (`--token-budget` / `--max-tokens`, default 4000 tokens on the CLI). `--limit` / `--offset` override the window; `shown`, `total`, `capped`, `has_more`, and `next_offset` are reported, with a `next` route when capped.
   - `--report`: the default budget is 16 KiB, shared equally by its five lists (modules, bridges, god files, dependency cycles, top symbols). Each list reports `(showing S of T)`; `--limit` / `--offset` window each list, and a capped list prints a `next: --report --limit=N --offset=M` route.

CLI and MCP adapters share transport-neutral `query::*` implementations and
canonical reports. The root-exported `Workspace` lives in `src/workspace.rs`
and loads shared SCIP-aware analysis; ranking is held in `Workspace.pr.ranks`.
MCP wraps the same XML/text in JSON-RPC `result.content[0].text`.
`Workspace.query_context(sources)` borrows the graph and ranks for the shared
`query::commands` registry; adapters do not clone them for dispatch.
`SourceReader: Sync` supplies source-query bytes through the shared registry.
`XmlWriter::finish` completes a report; `Write::flush` only flushes bytes.

Rust ingest records impl-block spans, concrete subjects, trait bounds, and
function-pointer field assignments without turning declarations or assignments
into call edges. Resolution uses actual call sites: source-spanned inline modules,
indexed module files, and lexical type identity select owners before trait
dispatch or callback invocation. Unproven or ambiguous owners decline rather
than binding to an unrelated same-named method. `--lego` uses the same Rust
trait/subject index and declaration spans for implementors and contract methods.

## Language Plugins

A language is a directory, `src/languages/<lang>/`, that holds everything specific to it:
`mod.rs` (its `LanguageSemantics` impl and helpers), `tags.scm` (the Tree-sitter query), and
optionally `grammar.txt` (a vendored C grammar: `files = parser.c scanner.c`, optional `src =`,
`include =`, `upstream = <repo> <branch> <subdir>`, `rev = <commit sha>` and `patch = <sed script>`,
read by `build.rs` and `scripts/update-grammars.sh`). The build fetches exactly `rev`, never a branch
head; `scripts/update-grammars.sh --bump [lang]` moves a pin to the current head, so grammar changes
are reviewed commits. Callers only see the `LanguageSemantics` trait, so shared
code never names a language. To add one, create the directory and add one line to
`register_languages!` in `src/languages/mod.rs`, which generates the `Lang` enum, the module
declaration and the semantics table. `src/languages/common/` is not a language: it holds the shared
helpers and vocabulary every plugin builds on (syntax and complexity kinds, flag facts, lint groups,
keyword fallback) and no language logic. `LanguageSemantics` also states each language's test-partner
naming (`test_partner_subject`, `supports_test_partners`); the universal rules are the defaults in
`src/languages/common/util.rs`. `tests/languages/contract.rs` checks tags, extension ownership, grammar
loading, hook pairs and registration for every language.

## Command Plugins

A command is one module, `src/commands/<name>.rs`, that holds everything specific to it: its clap `Args` (its primary
flag and only the modifier flags it uses), its handler, and its MCP tool descriptors. The module exports one
`pub static` `Plugin<Args>` (`name`, `options`, `selected`, `run`, `check`, `tools`). `SharedOption::Paged` is declared
only by commands whose output honors `--limit/--offset`. Helpers shared by several commands live in
`src/commands/common/<topic>.rs`, which is not registered. A flag that several commands read is declared once on
`TopModifierArgs` (`src/cli/args.rs`) and read through `cli.modifiers`; a flag only one command reads stays on its
owning module as a `pub` `Args` field. A plugin does not decode another plugin's `Args`; the few exceptions still in the
code are named in the commit that introduced this rule. The subcommands
`gate`, `metrics`, and `slice` are their own modules and flatten the owning `Args` structs. The CLI and MCP layers only
see the `Command` trait, so shared code never names a command. Plugin-specific rules live behind `Command` hooks:
`validate` (flag combinations), `refreshes_remote_checkout` and `run_default` (the default output, such as the `map`
plugin). `name()` and `options()` are required on every `Command`: `name` is the primary flag or subcommand without
`--`, empty for a plugin that is not a single command, and `options` lists the shared options it honors. Paging is
checked end to end by `tests/commands/paging.rs`, which runs each paged command in memory at `limit=1`. To add one, create the module and add one line to `register_commands!` in `src/commands/mod.rs`, which
generates the module declaration and the registry; the line order is CLI dispatch precedence. `src/commands/context.rs`
is not a command: it holds the shared CLI plumbing every plugin uses (`Paginated`, `OutputDialect`,
`ExecutionContext`, repo-analysis loading). `src/cli/` keeps only the core options (`Cli`, `TopModifierArgs`) and the
parse and run entry points.
Shared code receives the tool list as a parameter (`query::commands`, `mcp`), never by naming a plugin;
composition roots pass `crate::commands::tools()`. Metric analysis that a plugin also needs lives in
`src/metrics/`, so `src/metrics/` never imports `crate::commands`.

### Adding a command

1. Create `src/commands/<name>.rs`. A command with one flag and a report body is one `report_command!` declaration
   (`src/commands/plugin.rs`): its `name`, derive `Args` struct, `static` name, `options`, `tools`, the `run` mode
   (`report(render)`, `query(execute)` or `nav(mode)`) and the flag with its doc comment and `#[arg]`, then a
   `pub fn render` for the body. Any other command writes the derive `Args` (the primary flag plus any modifier only
   this command uses), a handler `fn(&Run, &Args) -> io::Result<()>` and `pub static NAME: Plugin<Args> = Plugin { .. }`.
2. Add `<name>::NAME,` to `register_commands!` in `src/commands/mod.rs` (position = dispatch precedence).
3. Use `run.report(|ctx, out| ..)` for the analysis and output dialect (`--json`, `--pretty`, `--legend`) and
   `run.page()` for the `--limit/--offset` window. Attach MCP tools in `tools:`.

### Shared options (`--limit` as the example)

A shared option reaches a command only through `Run`, and a command receives it only if it declares it in
`options`. To make a command honor `--limit/--offset`, list `SharedOption::Paged` and window its rows with
`Paged::new(rows, run.page(), cost)`, which derives the cap from the output budget unless `--limit` overrides it. A
command that does not declare it prints a stderr notice when the flag is passed (`unhonored_option_warning`).
`tests/commands/registry.rs` keeps that contract true for every registered command.

The same declaration also gives the command's MCP tools their parameters: the registry (`commands::tools()`) appends
the rows of each declared option to every tool of the command, so a tool never lists them in its own `params`.
`Paged` and `McpPaged` add `limit`/`offset` worded as reports, `PagedMatches` adds them worded as grep matches, and
`Budget(text)` adds `budget_tokens` worded as given. `CliPaged` windows the CLI report only; `McpPaged` exposes the
parameters for a tool whose report the CLI does not window.

Every tool also gets the envelope parameters `path`, `paths`, `legend` and `root` it does not declare itself
(`ENVELOPE_ROWS` in `query/commands.rs`). `query::commands::dispatch` honors `legend: "none"` by dropping the leading
schema comment of a string report, and carries a `path` subdirectory into `in_scope` for a tool that takes
`in_scope`. Elsewhere `path` and `paths` are accepted client hints. `root` is handled one layer up, in
`mcp::dispatch::handle_tools_call`: the server holds the active `Workspace` plus the previous one parked inside it
(`Workspace::select_root`; a `--scip` workspace outlives a plain one), so alternating between two roots does not
re-index. It re-indexes the active workspace when the working tree changed since it was built
(`Workspace::refresh_if_stale`). A `batch` takes one
`root` for all its queries; a `root` inside a query is an error.

To add another shared option (for example `--since`): add a variant to `SharedOption`, one field on `TopModifierArgs`,
and one gated accessor on `Run`; commands then opt in with one `options` entry and never read the flag directly.

## Lineage & Acknowledgments

This crate is a pure Rust port and rewrite of the groundbreaking [ripwire (C++23)](https://github.com/redhat-et/ripwire) static analysis engine, originally conceived and authored by **David Brewster** at **Red Hat Emerging Technologies**.

We express our deepest gratitude to the original creators and contributors at `redhat-et/ripwire` for pioneering the concept of "the ripgrep of AI context," developing the streaming XML protocol, formulating the pre-commit quality delta heuristics, and proving the immense power of deterministic, token-budgeted code intelligence for agentic software engineering.
