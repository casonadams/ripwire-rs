# Baseline (before the alignment work)

Measured on this repository at `5a97d03` with `target/release/ripwire-rs` and upstream `ripwire 0.6.9`.
Re-run the commands below to compare after a change.

## Conceptual retrieval (`scripts/retrieval-eval.py --budget 2000 --runs 1`)

52 questions across five splits. `top1`: first row is authoritative. `mrr`: mean reciprocal rank.
`pres`: an authoritative row is anywhere in the answer. `bytes`: median stdout per query.

| tool | top1 | hit@3 | hit@10 | mrr | pres | rec | rows | bytes |
|---|---|---|---|---|---|---|---|---|
| ripwire-rs | 0.27 | 0.42 | 0.69 | 0.387 | 0.79 | 0.69 | 18.5 | 8276 |
| ripwire 0.6.9 | 0.42 | 0.62 | 0.75 | 0.531 | 0.75 | 0.64 | 15.6 | 5903 |

Name-exact (`--eval=retrieval`, 60 sampled symbols): MRR 1.000, recall@1 100%.

## Default answer size, bytes (`ripwire-rs . <verb>`)

| verb | bytes | verb | bytes |
|---|---|---|---|
| `--communities` | 130885 | `--for=<concept>` | 18220 |
| `--deps` | 82388 | `--pack-task=<task>` | 17527 |
| `--lint` | 69252 | `--grep=row_bytes` | 16514 |
| `--clones` | 61098 | `--report` | 15456 |
| `--quality-panel` | 49779 | `--defs=run` | 2490 |
| `--impact=fit_rows` | 43233 | `--dead-code` | 1439 |
| `--hotspots` | 32179 | `--for=<name>` | 1188 |
| (default map) | 25166 | `--callers=<name>` | 1023 |
| `--impact=row_bytes` | 24659 | `--quality-delta` | 701 |

# After the alignment work

Same commands and repository state class (this repo, upstream 0.6.9). Ceilings for the sizes are in
`scripts/output-budgets.toml` and are enforced by `make size`.

| verb | before | after | verb | before | after |
|---|---|---|---|---|---|
| `--communities` | 130885 | 8958 | `--for=<concept>` | 18220 | 13579 |
| `--deps` | 82388 | 15946 | `--pack-task=<task>` | 17527 | 17994 |
| `--lint` | 69252 | 11832 | `--grep=row_bytes` | 16514 | 12066 |
| `--clones` | 61098 | 7905 | `--report` | 15456 | 15456 |
| `--quality-panel` | 49779 | 9208 | `--defs=run` | 2490 | 2501 |
| `--impact=fit_rows` | 43233 | 14726 | `--for=<name>` | 1188 | 1188 |
| `--hotspots` | 32179 | 8453 | `--callers=<name>` | 1023 | 1024 |
| (default map) | 25166 | 15723 | `ripwire-rs -h` | 18 KB (`--help`) | 2625 |

Conceptual retrieval at `--budget 2000` is unchanged in ranking and 19% smaller: top1 0.27, mrr 0.387,
pres 0.79, median 6706 B (was 8276 B).

## Conceptual ranking (second pass)

`scripts/retrieval-eval.py --budget 2000`, 52 questions. Three changes to the scoring evidence: the doc comment
above a definition (or the run of line comments there, which many codebases write instead) counts as evidence
at weight 8; a constant's own initializer text counts; a body's evidence weight falls with the square root of
its length above 30 lines, since a long body contains terms by chance.

| | top1 | hit@3 | hit@10 | mrr | pres | rec | median bytes |
|---|---|---|---|---|---|---|---|
| before | 0.27 | 0.42 | 0.69 | 0.387 | 0.79 | 0.69 | 8276 |
| after | 0.33 | 0.44 | 0.71 | 0.442 | 0.87 | 0.75 | 6686 |
| upstream 0.6.9 | 0.42 | 0.62 | 0.75 | 0.531 | 0.75 | 0.64 | 5903 |

Tried and dropped: weighting a constant with three or more matching terms like code (mrr unchanged, presence
down); the body-length exponent, doc weight and reference length are on a plateau (0.25 to 1 and 6 to 8 move mrr
by under 0.01, a 12 doc weight loses 0.04).

`--for` latency: `collect_for_lego` built a full resolution index per interface to print a count; it is now
built once and only when needed. `--for=<name>` 130 ms -> 61 ms, `--for=<concept>` 188 ms -> 77 ms, warm.

## Conceptual ranking (third pass): BM25 from the `bm25` crate

Reading upstream's source showed why hand-tuning plateaued: it ranks with BM25 over identifier subtokens
(one document per symbol: name x3, callee names, doc comment x2, own text; term-frequency saturation and
length normalization against the corpus average). Our scorer summed binary hits with cubic synergy, a
hand-set length reference and several cutoffs. Four coordinator signals added to that sum (a heavier call
weight, a per-extra-term bonus, boosting a symbol that calls top rows, demoting a helper under a higher
caller) all failed to beat mrr 0.440.

`src/query/lexical.rs` now builds those documents and scores them with the `bm25` crate (default features
off: `fxhash` is its only dependency). Counting and embedding run on rayon. A one-word lookup of an existing
name keeps its own small scorer (`score_exact_name`); the idf, synergy, evidence-mask and cutoff code is
gone.

| | top1 | hit@3 | hit@10 | mrr | pres | rec | ms/query |
|---|---|---|---|---|---|---|---|
| hand-weighted | 0.33 | 0.44 | 0.73 | 0.440 | 0.87 | 0.75 | 28.7 |
| BM25 (`bm25` crate) | 0.52 | 0.67 | 0.81 | 0.604 | 0.85 | 0.72 | 28.2 |
| upstream 0.6.9 | 0.42 | 0.62 | 0.75 | 0.531 | 0.75 | 0.64 | 88.9 |

Parameters sweep flat on 52 questions (k1 1.2 to 2, b 0.5 to 0.9, name copies 2 to 5, doc copies 1 to 4
move mrr between 0.57 and 0.61), so the crate defaults and upstream's field weights stand. Query-side plural
stemming was neutral (mrr 0.582, hit@10 0.77) and is not applied.

## Token price: measured with a tokenizer, not assumed

Every `est_tokens=` and `budget_tokens=` rested on "4 bytes per token". Measured with tiktoken (o200k_base, a
proxy: Claude's tokenizer is not public) over this repository and two TypeScript repositories, the answers cost
2.0 (attribute-dense `--callers`) to 3.2 (`--for`) bytes per token, median 3.0; the answers that take a token
budget centre on 2.75 (map 2.6, `--for` 2.7 to 3.0, pack-task 2.9, impact 2.75). `-h` prose is 4.3.

| answer | disclosed `est_tokens` / real, before | after |
|---|---|---|
| default map | 0.62 | 0.93 |
| `--for=<name>` | 0.51 | 1.18 |
| `--for=<concept>` | 0.46 | 0.96 |
| `--pack-task` | 0.72 | 1.06 |

Changes: one `serialize::est_tokens` / `budget_bytes` pair at 11 bytes per four tokens replaces the scattered
`/ 4` and `* 2 / 5`; the map prices the bytes it emits (header, body and inventory), `--for` the whole XML
instead of its signatures; `token_budget_bytes` is the price minus the map's frame (it had been a hand-fitted
`tokens * 2 - 1100`); an explicit byte budget now bounds the inventory tier too (`--pack-budget=12000` gave
19507 bytes, now 12434; the default map keeps its inventory); a `--for` answer budgets its ranked rows at half
the tokens because the one-hop, doc and tree sections are budgeted from the rows' budget. `make size` now
tokenizes the answers that disclose a price and fails outside 0.8 to 1.2 of the real count (needs `uv`; it
skips loudly without).

| `--token-budget=N` map, real tokens / N | 500 | 1000 | 2000 | 4000 | 8000 |
|---|---|---|---|---|---|
| before | 0.68 | 0.50 | 0.64 | 0.72 | 1.07 |
| after | 1.00 | 1.00 | 1.06 | 1.08 | 1.12 |

`--for` at its default 4000 tokens: 14422 bytes (5362 real tokens) to 11301 bytes (4138); at `--token-budget=2000`
real/asked 1.2 to 1.02; `--pack-task` 23943 to 16496 bytes (6000-token budget, 5641 real). Retrieval at budget
2000 is unchanged in quality (top1 0.52, mrr 0.604) at 6008 median bytes instead of 6599.

## Communities: Leiden from the `leiden-rs` crate

`--communities` and `--pack-task --partition` ran a hand-written, single-level Louvain pass.
`graphrs` was rejected first: its public `leiden()` seeds from the thread RNG, so the same graph gives
different communities. `leiden-rs` (rand, rustc-hash, thiserror; rayon off) takes a seed and also returns a
per-level hierarchy, which no verb reads since `--zoom` was removed. Same graph, similar community counts:

| repository | nodes | Louvain Q | Leiden Q | Leiden / Louvain time |
|---|---|---|---|---|
| this repo | 7701 | 0.679 | 0.876 | 6.2 ms / 1.2 ms |
| TypeScript service | 3181 | 0.729 | 0.910 | 1.6 ms / 0.1 ms |
| TypeScript web app | 19498 | 0.673 | 0.861 | 11.6 ms / 0.6 ms |
| Lua/JS app | 11611 | 0.650 | 0.825 | 8.3 ms / 1.8 ms |
| C++ tool (43600 nodes) | 43600 | 0.819 | 0.943 | 53 ms / 4.4 ms |

Output is identical across runs (checked three times). About 200 lines of Louvain, aggregation and
compaction code are deleted. The `lanes` strategy is renamed `leiden-community`.

## Resolver: oracle-measured, then fixed

`make resolution` builds a rust-analyzer SCIP index and scores the resolver against it (precision: a binding
we made names SCIP's target; recall: SCIP's targets we reached). Floors: precision 0.98, recall 0.96.

| Rust (this repo) | precision | recall | false name edges |
|---|---|---|---|
| first measurement | 0.9523 | 0.9620 | 340 |
| decline `count`, `parent`, `parse`, `bytes`, `peek` | 0.9762 | 0.9616 | 88 |
| std-named member call needs a proven receiver | 0.9832 | 0.9625 | 26 |

TypeScript against scip-typescript: 0.995 / 0.993 and 0.997 / 0.976 (unchanged by the Rust work).

## Language audit (36 languages)

Seven parallel audits (fixture, upstream comparison, real corpus) found and fixed: Go method arity counted the
receiver; ObjC receivers untyped; Java `record`/`@interface`, Kotlin `typealias`, Scala `enum`/`new`
undefined; Python bare builtins bound to same-named defs and decorators were not calls; Erlang BIFs bound to
same-named defs; JS `module.exports.x = function y` and `X.prototype.m = function n` named by the inner name;
Markdown fenced code and URLs became links; SQL had no table references; GraphQL had no fragments or types;
HCL blocks all named by their keyword; proto/Solidity/COBOL/Fortran missed references or definitions; C
headers parsed as C++; C++/CUDA destructors and `new` missing; Metal kernels unparsed; Zig return types
defined as functions and imports ignored; Dart constructors made every construction ambiguous (ambiguous
calls 6093 to 116 on a 1189-file app). Already correct: C#, Ruby, PHP, BrightScript, CUDA kernels.

Open, needing shared resolver or ingest work: C++ typed receivers across in-class and out-of-line methods;
JS/Swift/Go/Python typed receivers; Python method kind; JS namespace imports; Markdown heading links;
`definition.type` maps to `var` for enums and aliases.

Measured against compiler SCIP indexes, those gaps barely show on real code (precision / recall):
Python (scip-python) 0.998 / 0.981 on a service and 0.996 / 0.992 on a data-pipeline repo; C++ (scip-clang, 67
translation units) 0.996 / 0.960; TypeScript 0.995 / 0.993 and 0.997 / 0.976. The typed-receiver defects the
audits found on toy fixtures cost under one wrong edge in 200 on these corpora, so Rust (0.985 / 0.967) is the
only language with error worth chasing. C++ misses are mostly calls into a header guarded by `#error` (a
section file) and `os::open` style wrappers around libc names; Python misses are `build`, `fold` and
dunder-call dispatch.

Rust typing added: struct field types (`ctx.graph.gauges()`) and free-function return types, including
module-qualified paths (`build::make().m()`), type a receiver. Precision 0.9832 to 0.9845, recall 0.9625 to
0.9671.

Import tier: a bare call (no qualifier, not a member call) to a free function or macro now binds to the
candidate in a file the caller imports when no candidate is in the caller's own file; before, two same-named
definitions in different directories made it `declined` however the caller imported. This is what the 48
`mcp_tool!` misses were. It narrows only unscoped candidates, so trait methods and `Type::new` are untouched
(a first version without that limit added 43 wrong `sym_name` edges). Oracles, precision / recall before to
after: Rust 0.9845 / 0.9671 to 0.9848 / 0.9714; TypeScript 0.9948 / 0.9929 to 0.9948 / 0.9971 and
0.9973 / 0.9763 to 0.9974 / 0.9908; Python and data-pipeline unchanged at 0.998 / 0.981 and 0.996 / 0.994.

Still open:

- A Rust `test_*.rs` file outside a test directory is classified as a test (the universal `test_` stem rule,
  pinned by `tests/gate/affected.rs`), so `src/commands/test_gate.rs` and `src/gate/test_gate.rs` count as tests.
- Parity still runs against this repository's own tree and the installed upstream; a pinned fixture corpus
  needs about 23 ledger entries rewritten.
