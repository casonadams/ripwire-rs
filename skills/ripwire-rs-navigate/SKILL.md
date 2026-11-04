---
name: ripwire-rs-navigate
description: Trace callers, callees, blast radius, call paths and literals; root-cause bugs from symptoms, stack traces or regressions; combine graph filters with --graph-query.
allowed-tools: Bash, Read
---

<identity>
You trace code structure with ripwire and never present an unverified edge as fact.
</identity>

# Navigate with ripwire

`<dir>` = repo or subsystem. Add `--legend=compact` on every call after the first; the first answer already taught the legend.

## Pick the verb for the question

- **Who calls it, one hop** - `--callers=SYM` (a sanity check, not a safety judgment). `--callees=SYM` is the outgoing side.
- **Is it safe to change or delete X** - run `--impact=SYM` (transitive reach) and `--uses=SYM` (call/read/write/import sites). `--callers` alone misses both non-call references and depth. Rows carry `tested="1"`; the root gives `radius_tested=` and `radius_untested=`.
- **Body and callee signatures** - `--expand=SYM` or `--expand=path:name`.
- **Several definitions share a name** - `--defs=A,B` returns a `sel=` that pins one. Any selector also takes `@FILE:LINE`; `--at=FILE:LINE` gives the enclosing chain.
- **How does A reach B** - `--path=A,B` (directed). For N>2 symbols, or a pair `--path` calls unreachable, use `--connect=A,B,C` (undirected; finds shared callers).
- **Literal or pattern** - `--grep=STR` or `--regex=PAT`, with `--grep-context=N`. Config and string hits need `--grep-in=any`. Narrow with `--and=B` / `--not=C`.
- **Verify a claim** - `--verify='calls(A,B)'` (also `uses`, `unused`, `contains`, `defines`, `reaches`). `not-established` means the index cannot prove it, not that it is false.
- **Variable in one function** - `--slice=SYM:VAR`; add `--slice-flow=back|fwd` for transitive flow.

An `amb="K"` edge (K calls matched several definitions) needs a source read at that call site before you rely on it.

## Bug branches

Pick the branch that matches what you know.

- **Symptom, no location** - `--for="<symptom>"`, preferring high-`cx`, high-`in` hits. Then `--hotspots`; a symbol in both lists is the prime suspect. Then `--impact` on the top 2-3.
- **Known subsystem** - note the recurring `p=` directories in `--for`, then `--grep="ERROR_TEXT"` for the emit site and `--callers` upward.
- **Regression** - `--situ` (or `--situ=a.c,b.h`) gives blast radius, tests, and co-change partners missing from the diff. Then `--callers=SYM` and `--cochange=FILE` (`surprising="1"` = no static link).
- **Stack trace, sanitizer, compiler error** - `--from-trace=FILE` (or `-` for stdin). Frames map innermost-first; read rank 1. `--run-trace="make -j"` runs the command and maps its failure output; the exit code is disclosed.
- **Wrong value** - `--slice=SYM:VAR --slice-flow=back`, then read the statements it names.

Stop once source confirms one candidate. Slices and graphs are name-based and stop at the function boundary: use them to focus reading, not as proof. Pin the root cause with `--note-add="SYM: what went wrong"`.

## Combined filters: `--graph-query`

Use only when conditions must combine.

`ripwire-rs <dir> --graph-query='EXPR' --legend=compact`. Operators: `all`, `name("X")`, `kind(E,K)` (fn, method, cls, struct, iface, var, sec), `cx(E,N)`, `fanin(E,N)`, `file(E,"RE")`, `callers(E[,D])`, `callees(E[,D])`, `and`, `or`, `not`.

```
ripwire-rs <dir> --graph-query='and(callers(name("buildGraph"),2),kind(all,fn))' --legend=compact
ripwire-rs <dir> --graph-query='and(cx(all,15),file(all,"src/"))' --legend=compact
```

Results are ranked and capped at `--top-k` (default 200): compare `count=` with `shown=`. Closures use name-based edges. A malformed expression returns a parse error, never a partial answer.

<output_format>
### Navigation Trace
- **Target Symbol**: [name and file:line]
- **Direct Callers / Callees**: [1-hop, with tested= where present]
- **Transitive Blast Radius**: [impact reaches=, radius_untested=]
- **Root Cause or Flow**: [branch taken, evidence, file:line]
- **Safety Assessment**: [local vs architectural; note via=, amb=, declined=]
</output_format>
