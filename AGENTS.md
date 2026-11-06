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
  `--eval=retrieval` (baseline in `docs/BASELINE.md`), not by output volume.
- **A small agent-facing surface.** Each new flag, skill or doc page costs every agent that reads it.
  Prefer extending an existing verb. Do not add a verb without removing or folding another, and keep the
  always-loaded skill text short.
- **Measure, then change.** For any change to defaults, ranking, resolution or budgets, record before and
  after (`docs/BASELINE.md` numbers, `make size`, the retrieval eval) in the commit message.
- **Upstream is a reference, not the target.** `redhat-et/ripwire` is credited and compared, but ripwire-rs
  matches its facts only where they help an agent. Record deliberate differences in
  `scripts/parity-divergences.toml` with the reason; a "richer" entry that depends on larger output is a
  smell.
- **Use a proven crate before hand-rolling an algorithm.** BM25 (`bm25`) and Leiden (`leiden-rs`) replaced
  hand-written scorers and cut code while improving quality. Check determinism first (a seedable RNG), then
  the dependency footprint (`default-features = false`), then measure quality and latency against the code it
  replaces; adopt it when it is no worse on either.
- **Compilers are the oracle for resolution.** `make resolution` scores the call graph against a
  rust-analyzer SCIP index (`--eval=scip --scip=INDEX`): precision and recall floors live in
  `scripts/resolution-ceilings.toml`. For another language, index a real corpus (`scip-typescript`,
  `scip-python`, `scip-clang`) and score it before and after a change; a fixture alone does not show whether a
  gap matters on real code. A resolver change that adds wrong edges is rejected even if recall rises.
- **Token prices are measured, not assumed.** `est_tokens=` and `budget_tokens=` come from
  `serialize::est_tokens` / `budget_bytes`, calibrated with a tokenizer (`make size` checks the disclosed
  price against tiktoken within 0.8 to 1.2). Change the constant only with a new measurement.
- **Retrieval is BM25 over subtokens.** Tune with `scripts/retrieval-eval.py`; a parameter sweep that moves
  mrr by under 0.01 is noise on 52 questions, so do not ship it. New scoring signals must beat the baseline in
  `docs/BASELINE.md`.

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
- **A language change needs a fixture and a std-name case.** Cover a free call, a typed-receiver call, two
  same-named methods, and an in-tree name that collides with a builtin (`len`, `get`, `map`) called on a
  builtin and on an in-tree receiver. Add a name to a `std_*_names` list only on evidence from a real corpus:
  a listed name halves that symbol's PageRank prior and blocks unpinned cross-file binding. When unsure
  whether a call binds, decline it; a labelled name edge is acceptable, a wrong precise edge is not.
- **Edit a `tags.scm`, then `touch` the language's `mod.rs`.** The grammar is `include_str!`ed and the test
  binary can miss a tags-only change.

## Testing, linting, and verification

- **No lint suppressions**: Never add Clippy `allow` / `expect` attributes or
  CLI exclusions; remove any encountered in touched code and refactor instead.
- **Fast, in-memory public-API tests**: Place all tests in `tests/<mod>/<file>.rs`
  (each `tests/<mod>.rs` is a module of the single integration binary `tests/it.rs`; run one area with
  `cargo test --test it <mod>::`), never inline `#[cfg(test)]` in `src/`. Never
  perform disk I/O or network calls in tests; use in-memory slices (`< 500ms`
  per test, `< 15s` full suite).
- **Generic names only**: Findings from other codebases must reach tests,
  fixtures, docs, and commit messages under generic names (`lint_rules.py`,
  `user_accounts`); never copy their repo, path, symbol, or table names. The one exception is the corpus manifest
  `scripts/corpus/corpus.toml`, the only place repo URLs and commit SHAs may appear; the corpus docs name languages only.
- **Documentation sync**: `docs/REFERENCE.md` lists every CLI flag and MCP tool and is
  generated from the binary; run `make docs` after changing one (`make docs-check`
  fails when it is stale). Hand-written docs (`README.md`, `docs/`, `www/`,
  `man/man1/ripwire-rs.1`, `skills/`) carry only prose and examples: update them in the
  same commit when behavior or defaults change, and verify with
  `ripwire-rs . --doc-drift`. Do not copy flag or tool lists into them.
- **Required gate**: Run `make all` before finishing (enforces `clippy`,
  cognitive complexity `<= 15`, `--quality-delta` vs `git HEAD`, default answer
  size ceilings (`make size`), call-resolution ceilings (`make resolution`),
  generated reference freshness (`make docs-check`), tests (`make test`), upstream parity, and `fmt` as
  the final step). There is no line-coverage or CRAP gate: cognitive complexity bounds each function, and
  `ripwire-rs --test-gate` / `--affected` check that changed code is reached by tests through the call graph
  (reachability, not executed lines).
- **Fast loop**: `make quick` (check, then the integration tests) or `make t FILTER=resolve::` (cargo test, about 2 s
  for all 734) while editing. The gates that run the binary (`size`, `resolution`, `docs-check`, `parity`, `corpus-*`)
  build `target/fast` (`RELEASE_PROFILE`: release's optimizer, no fat LTO, about 8 s per edit, not 140 s).
  `build-release`, `bench` and `install` stay on `target/release`.
- **Gate hygiene**: Run `make all` on a clean, committed tree after `ripwire-rs . --quality-baseline`;
  a dirty tree or a stale baseline gives false parity and quality failures. Parity probe symbols are pinned in
  `scripts/validate-parity.py` (`PINNED_TARGETS`) because the ledger is measured on them; a ranking change
  that swaps the probes is a harness artefact, not a regression. `make size` needs `uv` for tiktoken and
  skips loudly without it.
- **Parallel work uses git worktrees**, each with its own CARGO_TARGET_DIR, and every edit path under its
  own worktree. Merge on `main`, rebuild, and run `make all` once. Never edit the main tree from a worktree.
  Each worktree needs its own `CARGO_TARGET_DIR`; run `cargo clean` on it before removing the worktree, and
  `make sweep` prunes stale artifacts.
