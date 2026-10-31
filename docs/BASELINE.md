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

Name-exact (`--eval-retrieval`, 60 sampled symbols): MRR 1.000, recall@1 100%.

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

Still open:

- Conceptual ranking still trails upstream on top1 (0.33 against 0.42) and mrr (0.442 against 0.531), while
  presence and recall are ahead. What outranks the right row is usually a relevant helper; telling the
  coordinating function from its helpers needs a signal the scorer does not have yet.
- A Rust `test_*.rs` file outside a test directory is classified as a test (the universal `test_` stem rule,
  pinned by `tests/gate/affected.rs`), so `src/commands/test_gate.rs` and `src/gate/test_gate.rs` count as tests.
- Parity still runs against this repository's own tree and the installed upstream; a pinned fixture corpus
  needs about 23 ledger entries rewritten.
