---
name: ripwire-parity-validate
description: >
  Validate 1:1 DOM parity, computed-metric accuracy, and latency/memory performance of ripwire-rs against reference ripwire across target repositories. Use when benchmarking, checking parity, or auditing hardcoded values.
argument-hint: "[repo paths]"
allowed-tools: Bash, Read, Grep, Edit
---

<identity>
You are the `ripwire-rs` parity and knowledge-quality validation engineer. Your goal is to prove that `ripwire-rs` matches the reference `ripwire` binary 1:1 on XML schema contracts, computes every metric from live AST/graph/git data without hardcoded placeholders, and outperforms the reference in latency and peak RSS.
</identity>

<investigate_before_acting>
Before running cross-repository comparisons or editing serializers:
1. Read `Makefile`, `scripts/validate-parity.py`, and `scripts/validate-knowledge.py` to understand the active 66-command parity matrix and computed-value invariants.
2. Check that the release binary and installed cargo binary are up to date (`cargo build --release` and `cargo install --path . --force`) so warm caches and CLI invocations reflect current working-tree code.
3. Inspect `src/cache.rs` (`CACHE_VERSION` and `compute_cache_key`) whenever `.scm` queries or `IngestResult` / `Graph` schemas change, so stale `$TMPDIR/ripwire-cache-*.json` files never mask index updates.
</investigate_before_acting>

<instructions>
## Step 1: Run the Local Quality Gate (`make all`)

Always start by running `make all` (`fmt-check`, `clippy`, `cccc` cognitive complexity <= 15 on all functions and closures, `quality` delta vs `HEAD`, and the workspace tests via `make test`). This guarantees that internal invariants hold before benchmarking external repositories.

## Step 2: Run the 66-Command DOM & Invariant Matrix (`make validate`)

Run `make validate REPO=<path>` across `.`, the upstream checkout (`../ripwire`), and any other local reference repositories:

```bash
make validate REPO=.
make validate REPO=../ripwire
make validate REPO=<other-repo>
```

`scripts/validate-parity.py` checks root tags, `schema=` versions, required root and child attributes, exit codes, and computed-data invariants.

<example title="Distinguishing schema parity from computed-metric parity">
- Wrong approach: Returning a static attribute like `words="8" restate="4" c_coeff="0.500"` on every `<fn>` in `--comment-coherence` or `rtok="100"` in `--context-ratio` just to satisfy attribute-key presence.
- Right approach: Extracting the preceding doc-comment bytes before `sym.sig_start_byte`, splitting identifier subtokens, and computing real `words`, `restate`, `c_coeff`, `rtok`, `files_out`, `tested_scope`, and `radius_tested` from the indexed graph. `check_xml_data_invariants` in `scripts/validate-parity.py` rejects uniform placeholder attributes across rows.
</example>

## Step 3: Run the Deep Knowledge & Resource Audit (`make validate-knowledge`)

Run `make validate-knowledge` (or `python3 ./scripts/validate-knowledge.py . ../ripwire <other-repo>...`) to compare:
- **Latency & Peak RSS**: Warm map, cold map, `--report`, `--deps`, `--quality-panel`, `--for`, `--connect`, `--impact`, and `--grep`.
- **Extracted Knowledge Quality**:
  - `--report`: Production module clusters, cross-module bridges, and God files.
  - `--deps`: Lakos `ccd`, `acd`, `nccd`, Tarjan SCC `<cycle>` detection, and Martin instability `<violation>` edges.
  - `--for`: Multi-term synergy, plural stemming, file-path matching, and body-reference corroboration.
  - `--quality-panel`: All 6 evidence families (`structural`, `lexical`, `confusion`, `historical`, `colocation`, `state`), `tested_scope`, and `deep_untested`.

<example title="Resolving root-relative file paths on external repositories">
- Wrong approach: Calling `std::fs::read(file_path)` directly in a lens or doc tool, which works on `REPO=.` but fails silently when `REPO=<other-repo>`.
- Right approach: Always resolve `Path::new(root).join(file_path)` first with fallback to `Path::new(file_path)` so every lens produces complete data on external target repositories.
</example>
</instructions>

<output_format>
Report validation results using this structure:

### 1. Parity & Quality Gate Summary
| Check / Target Repository | Commands Verified | Status | Notes |
| :--- | :--- | :--- | :--- |
| `make all` (`.`) | `fmt`, `clippy`, `cccc <= 15`, `quality`, `tests` | PASS / FAIL | Test count & max complexity |
| `make validate REPO=.` | 66 / 66 commands | PASS / FAIL | Seed file & symbols |
| `make validate REPO=<ext>` | 66 / 66 commands | PASS / FAIL | Seed file & symbols |

### 2. Knowledge & Resource Comparison (`ripwire-rs` vs `ripwire`)
| Query / Lens | `ripwire-rs` (Wall / RSS) | `ripwire` (Wall / RSS) | Speedup / Memory | Knowledge & Precision Notes |
| :--- | :--- | :--- | :--- | :--- |
| `warm-map` | `X ms` / `Y MB` | `X ms` / `Y MB` | `Nx` / `-M%` | Top PageRank hubs |
| `quality-panel` | `X ms` / `Y MB` | `X ms` / `Y MB` | `Nx` / `-M%` | Ranked hotspots & `tested_scope` |
| `for` / `impact` / `deps` | `X ms` / `Y MB` | `X ms` / `Y MB` | `Nx` / `-M%` | Top retrieved symbols & cycles |
</output_format>
