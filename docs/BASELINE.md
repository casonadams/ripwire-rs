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
