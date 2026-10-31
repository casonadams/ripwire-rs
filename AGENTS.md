# Repository instructions

## The problem this repo solves (read first)

An agent in a repository answers "who calls this, what breaks, what do I test, did I make it worse" by
grepping and reading whole files. That burns context and lowers accuracy. `ripwire-rs` exists to give the
**smallest structural answer that is right**, and to say honestly how far to trust it. Two jobs:

1. **Find**: orient and navigate without reading the repo (`--for`, `--callers`, `--callees`, `--impact`,
   `--uses`, `--defs`, `--expand`, `--grep`).
2. **Check**: after a change, say what it affects and whether it made things worse (`--situ`,
   `--test-gate`, `--affected`, `--edit-check`, `--quality-delta`).

Everything else is a secondary lens. A feature belongs only if it makes a Find or Check answer smaller,
more correct, or more honest.

- **Fewer tokens is the product.** A default answer should cost an agent less than the grep and reads it
  replaces. Every default answer has a byte ceiling in `scripts/output-budgets.toml`, enforced by
  `make size`. Lists are budgeted (8 KiB each; a secondary list inside a report gets a quarter). More
  facts or bigger output is not a win by itself; parity with upstream is never a reason to grow an answer.
- **Honest before confident.** A guess is labelled (`via="name"`, `declined`, `unresolved`, `counts_floor`);
  an unproven edge is never counted or shown as proven. A wrong answer stated with confidence is the worst
  outcome; a smaller labelled answer is better than a larger unlabelled one.
- **Correctness of resolution is the ceiling.** Every verb inherits the call graph. Improve resolution
  recall and precision before adding verbs. Measure with `scripts/retrieval-eval.py` and
  `--eval-retrieval` (baseline in `docs/BASELINE.md`), not by output volume.
- **A small agent-facing surface.** Each new flag, skill or doc page costs every agent that reads it.
  Prefer extending an existing verb. Do not add a verb without removing or folding another, and keep the
  always-loaded skill text short.
- **Measure, then change.** For any change to defaults, ranking, resolution or budgets, record before and
  after (`docs/BASELINE.md` numbers, `make size`, the retrieval eval) in the commit message.
- **Upstream is a reference, not the target.** `redhat-et/ripwire` is credited and compared, but ripwire-rs
  matches its facts only where they help an agent. Record deliberate differences in
  `scripts/parity-divergences.toml` with the reason; a "richer" entry that depends on larger output is a
  smell.

## Core mission: Maximum intelligence in the smallest token footprint

`ripwire-rs` exists to replace blind `grep` and whole-file reads with deterministic, token-budgeted codebase intelligence across 36 languages, delivering the exact symbols, blast radius, and safety verification an agent or developer needs in one turn.

- **Seven core workflows**: Every feature serves one of (1) *Orientation & Scoping* (`--top-k`, `--for`, `--tree`), (2) *Navigation & Blast Radius* (`--callers`, `--impact`, `--uses`, `--connect`, `--slice`, plus zero-flag embedded SQL table lineage), (3) *Pre-Commit & PR Safety Gates* (`--edit-check`, `--affected`, `--test-gate`, `--situ`), (4) *Code Health & Architecture* (`--quality-delta`, `--quality-panel`, `--hotspots`, `--clones`, `--cochange`, `--deps`, `--dmm`), (5) *Deep Inspection & Context Delivery* (`--expand`, `--outline`, `--compress`, `--from-trace`, `--batch`, `--verify`), (6) *Agent & CI Integration* (`ripwire-rs mcp`, `--lsp`, `--add-skills`, `--scan-skills`, `--help-task`), or (7) *Multi-Agent Fleet Orchestration* (`--pack-task --partition=N`, `--plan-lanes=N --task=GOAL`, `--plan-lanes --brief=FILE`, `--merge-scout`, `--handoff`, `--notes`, `--note-add`).
- **Present-only serialization**: Omit zero/unit default attributes (e.g.,
  `ccx="0"`, `in="0"`, `churn="1"`, `amp="1"`), empty wrapper tags, and repeated
  per-row pagination hints. Place window metadata (`shown=`, `total=`,
  `capped=`, `next=`) once on the enclosing element.
- **Noise-free ranking**: Filter out non-callable noise, test/fixture helpers,
  language builtins, and prose backticks before ranking or counting. Partition
  unresolvable references into `unresolved`, `external`, and `declined`.
- **Actionable context in one turn**: Bundle the highest-value implementation
  body, resolved callee signatures (`<calls>`), architectural layer tags
  (`layer=`), and causal def-use order (`--slice`) directly into responses.

## Performance and code structure

- **Latency targets**: `< 50ms` warm on medium repos (~10k symbols), `< 500ms` on
  100k+ symbol monorepos, and strictly faster than upstream C++ `ripwire`
  (`make parity`, `make validate-knowledge`, `make bench`).
- **Deterministic, zero-buffer pipeline**: Sort crawled paths lexicographically
  and use deterministic tie-breaking everywhere (never leak hash iteration order).
  Keep hot-path work on interned `Spur` / `NodeId` integers and contiguous CSR
  graphs. Stream XML and JSON directly to `io::Write` sinks (`XmlReport`,
  `XmlEscapeExt`) rather than building intermediate `String`s in loops. Walk git
  history in-process via `gix`; never spawn `git` subprocesses in loops.
- **Idiomatic signatures**: Use natural parameter lists or domain context
  structs; never pack parameters into tuples (`(a, b): (&str, &str)`) to bypass
  argument limits.

## Tree-sitter languages

- **Capture vocabulary**: `src/languages/<lang>/tags.scm` must only use the
  standardized `@name`, `@definition.*`, `@reference.*`, and `@import.path`
  captures (never language-prefixed capture tags). Anchor `.` on the primary
  identifier for multi-segment selectors so each method emits a single symbol.
- **Semantics & enum stability**: Implement language rules via
  `LanguageSemantics` in `src/languages/`. Append new `enum Lang` variants
  before `Lang::Unknown`; never reorder existing variants (serialized in cache).

## Testing, linting, and verification

- **No lint suppressions**: Never add Clippy `allow` / `expect` attributes or
  CLI exclusions; remove any encountered in touched code and refactor instead.
- **Fast, in-memory public-API tests**: Place all tests in `tests/<mod>/<file>.rs`
  (rooted by `tests/<mod>.rs`), never inline `#[cfg(test)]` in `src/`. Never
  perform disk I/O or network calls in tests; use in-memory slices (`< 500ms`
  per test, `< 15s` full suite).
- **Generic names only**: Findings from other codebases must reach tests,
  fixtures, docs, and commit messages under generic names (`lint_rules.py`,
  `user_accounts`); never copy their repo, path, symbol, or table names.
- **Documentation sync**: When changing CLI flags, subcommands, defaults, or MCP
  tools, update `README.md`, `docs/`, `www/`, `man/man1/ripwire-rs.1`, and
  `skills/` in the same commit, and verify with `ripwire-rs . --doc-drift`.
- **Required gate**: Run `make all` before finishing (enforces `clippy`,
  cognitive complexity `<= 15`, `--quality-delta` vs `git HEAD`, default answer
  size ceilings (`make size`), test coverage CRAP `<= 30`, upstream parity, and
  `fmt` as the final step).
