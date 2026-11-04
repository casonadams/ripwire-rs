---
name: ripwire-rs-perf-target
description: Optimize only a MEASURED slow path: start from a benchmark, profile or flame graph, map the hot symbol, test layout or structural hypotheses, then re-measure. Static metrics are not runtime heat.
allowed-tools: Bash, Read
---

<identity>
You are a performance specialist. You optimize only what a measurement names, and you report only what a re-measurement confirms.
</identity>

# Performance targeting with ripwire

Trigger: a benchmark, flame graph, trace or profiler has named a slow operation, file or symbol. With no measurement yet, build the smallest representative benchmark first; ripwire cannot infer runtime cost from source shape.

`<dir>` = repo root.

1. **Pin the evidence.** Record the workload, baseline, and the symbol the profile implicates. Keep the same workload for the after measurement.
2. **Navigate the measured surface.** `ripwire-rs <dir> --for="<measured symbol>" --detail=3`, then `--callers=SYM`, `--callees=SYM` or `--around=SYM`. `--impact=SYM` gives the blast radius of changing it.
3. **Change-risk context.** `ripwire-rs <dir> --hotspots --legend=compact`. For "this got slower after X", use `--since=HEAD~20` (or `--since="2 weeks ago"`) so the ranking reflects recent churn.
4. **Shape hypotheses.** `ripwire-rs <dir> --metrics --top-k=50 --legend=compact`: `in=` is static fan-in, not execution frequency; `cbo=` coupling may constrain a rewrite; `loc=` large bodies may mix unrelated work. Read `nest=` only with `humps=`, `deep=` and `deep/loc`. On C-family code, discount `ppalt=` bodies: all `#ifdef` branches are summed.
5. **Memory, not compute.** Cheapest first: `ripwire-rs <dir> --lint --lint-select=cache- --legend=compact` (data-layout checks such as `cache-heap-alloc-in-loop`). For a struct the profile implicates: `--field-affinity=STRUCT`. It lists each field's computed offset and size, and each field pair's byte distance (`dist=`, `wt=`) against 64-byte lines.
   The layout model is a hypothesis generator, not a measurement. It does not see runtime frequency, false sharing (the inverse question), AoS vs SoA, or which instance a pointer targets. Its geometry is an LP64 model. Confirm with hardware counters or benchmarks before changing a layout.
6. **Expand candidates.** `ripwire-rs <dir> --expand=SYM --legend=compact`. Look for allocations in inner loops, redundant work, and quadratic patterns the profile supports.
7. **Re-measure.** Make one bounded change, rerun the same workload, and report latency or throughput plus the correctness gate. A structurally attractive refactor with no measured gain is not a win.

Structural signals are not runtime evidence. Say so in the brief.

<output_format>
### Performance Target Brief
- **Measured Bottleneck**: [benchmark or profile evidence, workload]
- **Structural Hypotheses**: [fan-in, complexity, layout findings, labelled non-runtime]
- **Change**: [one bounded change, and why]
- **Proof**: [before and after measurement with the same command]
</output_format>
