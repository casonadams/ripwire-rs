---
name: ripwire-rs-navigate
description: >
  Navigate code symbols: direct callers, callees, transitive blast radius, execution paths,
  and literal searches. Use when tracing symbol impact before renaming or modifying code.
allowed-tools: Bash, Read
---

<identity>
You are a code navigation and dependency tracing expert. Your goal is to trace callers, callees, transitive blast radii, and execution paths without hallucinating unverified connections.
</identity>

<investigate_before_navigating>
Before modifying a named symbol:
1. Query direct 1-hop callers: `ripwire-rs <dir> --callers=SYMBOL --legend=compact`
2. Query outgoing dependencies: `ripwire-rs <dir> --callees=SYMBOL --legend=compact`
3. Check transitive blast radius: `ripwire-rs <dir> --impact=SYMBOL --legend=compact`
</investigate_before_navigating>

<instructions>
# Navigate with ripwire

> Nearest neighbours:
> • You DON'T know the symbol yet — orienting on a whole repo/subsystem → **ripwire-rs-orient**.
> • You have a symptom and need to FIND the buggy code → **ripwire-rs-find-bug**.
> • Your question needs to COMBINE conditions (cx + fanin + file + reachability) → **ripwire-rs-graph-query**.
> • "Is this diff safe to merge" (not "is it safe to touch this symbol") → **ripwire-rs-change-check**.

`<dir>` = the repo or subsystem. Calls are warm after the first parse. Tracing across a split
service+client checkout — pass every root, `ripwire-rs dir1 dir2 --impact=SYM` — the merged graph carries the
cross-root evidence edges (include/import/FFI) a single-root call would never see.

## Keep the legend on the FIRST call, drop it on every call after

Every XML verb prefixes its answer with a legend defining the attributes. You need it once. After that
you are paying for prose you have already read — and the cost is worst on exactly the verbs you call
most, because the legend is a fixed size while these answers are small. Measured on ripwire's own repo:

| verb | saved by `--legend=compact` |
| --- | --- |
| `--callers=SYM` | **~70%** |
| `--uses=SYM` | **~65%** |
| `--impact=SYM` | **~50%** |
| `--affected=F1,F2` | **~67%** |
| `--for="..."` | ~4% |

**The payload is byte-identical** — the entire difference is legend prose. (Percentages, not byte counts:
an exact byte total goes stale the next time anyone edits a legend, and a stale number in a skill is worse
than no number. `ripwire-rs --help` carries the range, and a gate holds it to it.) So:

- **Orientation** (`--for`, `--pack-task`): leave the legend on. It is ~4% there, and it is where you
  learn what `amb=`, `cx=` and `counts_floor=` mean. Reading a map whose legend you skipped is how
  confident misreadings happen.
- **Every navigation call after** (`--callers`, `--uses`, `--impact`, `--expand`): add
  `--legend=compact`. Across a seven-call session that is ~34% fewer bytes, ~2,900 tokens.

The MCP server already defaults to compact for this reason — the tool description carries the schema, so
the legend would be redundant on every call. The CLI defaults to `full` because a human reading one map
needs it. If you are an agent making repeated calls, you are the case the CLI default is not tuned for.

## `--callers` is 1-hop — don't let it answer "is it safe to change X?"

`--callers=SYM` gives direct in-edges only. It under-counts on purpose (it's the *cheap* verb) — a caller
two hops away, a read/write that never calls SYM, or an `#include` that pulls it in all fall outside a
1-hop answer. Match the verb to the question:

- **"Is it safe to change/delete X?"** → `ripwire-rs <dir> --impact=SYM --legend=compact` (transitive blast radius: everything
  that transitively reaches SYM through the call graph, not just direct callers) **+**
  `ripwire-rs <dir> --uses=SYM --legend=compact` (the resolvable use-sites by role — `call|read|write|import|extends` — file:line; catches
  non-call references `--callers` never sees, e.g. a struct read or a header import). Run both — `--impact`
  gives depth (the call chain), `--uses` gives breadth (kinds of reference). `--callers` alone is the wrong
  tool for this question; reach for it only when you already know the change is local.
- **"Who breaks, AND which of those a test already reaches"** — `--impact`/`--callers`/`--callees` rows
  carry `tested="1"` when an indexed test transitively reaches that row (omitted, never a literal `0`, when
  none does); `--impact`'s root adds `radius_tested=`/`radius_untested=` over its transitive reach,
  `--callers`/`--callees`'s root adds `hop_tested=`/`hop_untested=` over their 1-hop count — one call
  answers both "what breaks" and "what's covered" instead of a second `--test-gate` round-trip.
- **"Who calls this, one hop"** (quick sanity check, not a safety judgment) → `--callers=SYM`.
- **"Where is this VARIABLE defined and used inside one function"** → `--slice=SYM:VAR` — per-line
  def/use rows (declaration/assignment/param vs read) inside the one resolved definition; bare
  `--slice=SYM` lists the sliceable locals first. Name-based and intra-procedural — the legend states
  the limits — so it answers "what touches this variable here" without reading the whole body.
- **"Where did this variable's VALUE come from / what does it flow into"** (still inside one function)
  → add `--slice-flow=back|fwd|both` — the transitive cross-statement data-flow slice over
  reaching-definition def-use edges: `back` = the statements whose values feed the seed variable,
  `fwd` = the statements its value reaches; each flow row carries the variable (`v=`), the BFS depth
  (`d=`) and the line it was reached from (`f=`). `--slice-depth=N` bounds the walk (default 8; a
  bound that cuts is disclosed as `flow_truncated="1"`). Stops at the function boundary by design —
  the inter-procedural half is `--callers`/`--impact`. Data dependence only — the guard deciding
  whether a def executes is never a row.
- **"I have a FILE:LINE, not a name"** (a compiler error, a diff hunk, a stack frame) →
  `ripwire-rs <dir> --at=FILE:LINE --legend=compact` — the enclosing-definition chain at that location, outermost→innermost;
  `sym=` names the innermost. The SAME seed composes into the symbol selectors as `@FILE:LINE`
  (`--callers=@src/f.rs:120`, `--expand=@…`, `--edit-check=@…`) and resolves to that innermost definition, pinned by
  location so a shared name cannot bind the wrong one. A location on a blank top-level line, an ambiguous path, or
  no definition spanning the line is refused with a specific diagnosis, never guessed. `--slice` does not take seeds.
- **"Trace a FLOW: how does A reach B"** → `--path=A,B` (shortest call-path) or `--around=SYM
  [--around-depth=2]` (bounded neighborhood). Not `--for` — `--for` returns a ranked *set* of relevant
  signatures for a task, it does not trace a path between two named points.
- **"N task symbols — how do A, B and C RELATE, which intermediaries join them?"** →
  `ripwire-rs <dir> --connect=A,B,C --legend=compact [--connect-radius=N]` — the minimal connecting subgraph: your terminals,
  the fewest joining intermediaries (with signatures), and the call edges in true caller→callee direction.
  Reach for `--connect` over `--path` in two cases: **N>2 symbols** (`--path` only ever takes SRC,DST — it
  has no notion of a third point), or **a pair `--path` calls unreachable**. The search is undirected, so it
  finds the *shared caller* joining two symbols — the most common way task symbols relate, which a directed
  `--path` can never see (`--path=A,B` says `reachable="0"` even when `main` calls both). Symbols that can't
  meet within the radius appear honestly in `<unconnected>`.

## Trace the call graph / locate code

- **Who calls / what it calls** — `ripwire-rs <dir> --callers=SYM --legend=compact` · `ripwire-rs <dir> --callees=SYM --legend=compact`
- **The resolvable use-sites of a name** (role=call|read|write|import|extends, file:line; `counts_floor="1"` — the count is a floor) — `ripwire-rs <dir> --uses=SYM --legend=compact`
- **Who reads/writes a MEMBER VARIABLE** (`t="field"` symbols; per-site owner resolution, `owner_candidates=K` where several owners could match, never a silent pin) — `ripwire-rs <dir> --uses=Owner.field --legend=compact`
- **Transitive blast radius** — `ripwire-rs <dir> --impact=SYM --legend=compact`
- `--uses` lists all indexed sites unless `--limit=N` or `--offset=M` requests a window; continue at `next_offset`.
  `--impact` keeps unbound candidate calls in separate `<dc>` rows. They are not edges or reached symbols;
  `dc_shown` of `declined_calls` says how many are printed; raise `--token-budget` to see every candidate site.
- Python, JavaScript/TypeScript, and Rust index bare non-local variable reads.
  Grep's `reads=` / `reads_at=` are indexed sites, not text estimates; locals,
  assignment targets, and unsupported binding forms are not reconstructed.
- **Neighborhood** (bounded k-hop ego graph) — `ripwire-rs <dir> --around=SYM --legend=compact [--around-depth=2] [--around-fanout=32]`
- **How does X reach Y** (shortest call-path) — `ripwire-rs <dir> --path=SRC,DST --legend=compact`
- **How do N symbols relate** (minimal connecting subgraph, shared-caller joins) — `ripwire-rs <dir> --connect=A,B,C --legend=compact [--connect-radius=N]`
- **Verify a CLAIM in one call** — "does X really call Y?", "is Z ever used?", "does this file contain/define A?" →
  `ripwire-rs <dir> --verify='calls(A,B)' --legend=compact` (also `uses(SYM)` / `unused(SYM)` / `contains(FILE, "LIT")` /
  `defines(FILE, SYM)` / `reaches(SYM, "FILE")`): one three-valued verdict with the evidence inline —
  `confirmed` (witness printed) · `refuted` (only with complete evidence; a clean literal-scan no carries
  `complete="1"`) · `not-established` (`limit=` names the floor: dynamic dispatch and string-keyed references
  are invisible to the index, so this verdict is honest "the index cannot prove it", never "false").
  Replaces the grep-then-read chain you would otherwise run to check the claim yourself.
- **Find a literal / regex / structural shape** —
  `ripwire-rs <dir> --grep=STR --legend=compact` (literal + enclosing symbol) ·
  `ripwire-rs <dir> --regex=PAT --legend=compact` ·
  `ripwire-rs <dir> --match='(<tree-sitter query>)' --legend=compact` (e.g. `(call_expression function: (identifier) @c)`) ·
  **`ripwire-rs <dir> --pattern='foo($X, ...)' --legend=compact`** — the same structural search written in CODE instead of in
  node kinds, so you do not have to know whether this grammar calls it `call_expression`, `call`,
  `method_invocation` or `invocation_expression`. `$NAME` binds one node (repeat it and both sites must
  match), `$_` binds nothing, `...` (or `$$$`) is an ellipsis over siblings. ONE pattern searches every
  served language at once — c, cpp, objc, java, csharp, javascript, typescript, python, go, rust, swift —
  and `grammars=`/`shapes=` on the result name which ones it resolved for and what node kind it became in
  each. Reach for `--pattern` when you can WRITE the shape and for `--match` when you need a constraint the
  pattern language cannot express (a field name, a `#match?` predicate). Ruby, bash and the data tiers are
  refused by name, never answered with a zero.
  — add `--grep-context=N` (or `--grep-before=N`/`--grep-after=N`) for ripgrep-style N lines of source
  around each hit, so you see the call site's shape without a follow-up `--expand`.
  When one grep answers a two-term question ("cache staleness check for the MCP index"), narrow it in the
  SAME call instead of grepping again and eyeballing the intersection: `--and=B` (repeatable) keeps only
  hits where B is ALSO present, `--not=C` (repeatable) drops hits where C IS present — literal-only, so
  they pair with `--grep=`, not `--regex=`. `--grep-scope=line` (default) requires the extra term on the
  SAME matched line; `--grep-scope=file` widens that to anywhere in the same file.
  Hits are SPAN-TIERED: comments are suppressed while code hits exist; string hits ride along only
  when fewer than eight code hits exist. With no code, comments and strings are served together.
  `suppressed_comment=`/`suppressed_string=` disclose withheld mentions. Off-index text hits are
  listed separately in `<unindexed>`. When the mention IS what you were
  after (an error-message string, a design note), re-ask with `--grep-in=any` for every tier.

`--callers`/`--callees` answer from the call graph directly — no separate index step. Edges are name-based:
a high-rank symbol with no callees may be a dispatch hub (virtual/callback/macro), not a leaf — read it.
A `t="modscope" n="&lt;file-scope&gt;" p="FILE:1"` row is that file's module scope (top-level statements and
anonymous callback bodies): a real caller with no body to expand — its `sites_at=` lines are the call sites.

**What a high `amb=` should CHANGE about your next action**: `amb="K"` on a symbol means K of its outgoing
calls matched more than one same-named definition and the resolver guessed. Before relying on an ambiguous
edge to judge safety or trace a flow, inspect the source at that call site (`--expand=SYM`) to confirm which
definition it resolves to.
## Deep-dive ONE symbol — understand it before editing

When you need to understand a specific function/class/concept in full (its body, contract, and rationale):

1. **Full body + callee signatures** — `ripwire-rs <dir> --expand=SYM --legend=compact`
   `<bodies>` with SYM's full source in CDATA and a `<calls>` block of inline one-line signatures for
   everything it calls — read the body with the callee signatures beside it. No ranked map rides along (orient
   ONCE per session with `ripwire-rs <dir>`, then look symbols up lean); a bare name with several definitions
   serves them all, still map-free — `map_next=` on the root is the call that prints the map if you skipped
   orienting, and `unserved_next=` is the call serving the definitions a byte budget cut (the first `unserved_listed=` of
   them when that attribute is present; `unserved_total=` is the true count). (No `<doc>` block
   here; SYM's own doc-comment is in the CDATA body if it sits inside the definition — otherwise read the
   source lines just above `l=`.)
   About to Edit what you just expanded? If your edit tool needs a fresh native Read of the file first,
   the served body doesn't satisfy that — but the
   read doesn't need to start at line 1: Read at `l=`, not the whole file. Same for a `--for` hit before
   you've expanded it — its `<d>` row carries `l=` too. Over MCP, skip the Read requirement altogether —
   see ripwire-rs-mcp's edit verbs.
2. **Who calls it** — `ripwire-rs <dir> --callers=SYM --legend=compact` → `<callers of="SYM" count="N">` with type, name,
   file:line. Callers reveal SYM's contract from the outside — expected preconditions.
3. **What it calls** — `ripwire-rs <dir> --callees=SYM --legend=compact` → cross with the `--expand` body to understand the flow.
4. **Design docs that mention it** — `ripwire-rs <dir> --mentions=SYM --legend=compact` → `<mentions of="SYM" defs="D" docs="N">`
   listing markdown files that backtick-name SYM: the design decisions and rationale around this symbol.
5. **Neighborhood** (optional, when callers+callees don't close the picture) — `ripwire-rs <dir> --around=SYM --legend=compact
   [--around-depth=2]`.
   → **Explanation:** what SYM does (from the body + `<calls>`), who calls it and why, what it coordinates,
   and any design rationale (from `--mentions`). Note `amb="K"` if call edges are ambiguous — verify in source.

## Editing a STRUCT whose bytes cross a boundary — `--layout=STRUCT`

For a plain function, the deep-dive above is the whole story. For a **struct/class whose bytes are a
contract with something outside this compiler** — a GPU uniform block, a wire/IPC/file-format record, a
type mirrored into a stub or a second checkout — the question before you edit is not "who calls it" but
"what is the byte layout, what pins it, and who else declares it":

```bash
ripwire-rs <dir> --layout=AudioUniforms --legend=compact         # file:name disambiguates, like --around/--lego
```

One call gives three things: the fields in declaration order with **computed** offsets/sizes and every byte
of padding made explicit; every `static_assert` in the index that mentions the type, with `agree="0"` when
a `sizeof(X)==N` tripwire contradicts the computed size; and **every same-name definition**, compared field
by field. It **exits 2** when the contract is broken — `mirror="mismatch"` (`kind="drift"`: two populated
definitions disagree) or a contradicted tripwire — so it works as a pre-commit check, not just a report.
`kind="stub"` (an empty placeholder) and `kind="spelling"` (`simd::float4` vs `float4` — the two arms of
one `#ifdef`) are reported but exit 0.

**Believe the caveats.** The offsets are a MODEL, not the ABI: a lexical walk under standard-layout
assumptions on a 64-bit LP64 target. When it cannot see the truth it says `modeled="0"` with a named
`<caveat>` instead of printing a number — `#pragma pack`, bitfields, virtuals, base classes, nested or
anonymous aggregates, `#if`-conditional members, templates, and any field type it cannot size. One unsized
field un-places every field after it. A `modeled="1"` number that agrees with the type's own
`static_assert` is trustworthy; a `modeled="0"` def is telling you to read the source.

## `--query` (lexical) vs `--for` (task lens)

- `ripwire-rs <dir> --query="terms" --legend=compact` — **pure BM25 relatedness**: the map re-ranked by lexical match against
  your terms, nothing else. Use it when you want *what mentions these words*, uncolored by importance —
  chasing a domain term, an error string's neighborhood, a concept's vocabulary.
- `ripwire-rs <dir> --for="task in words"` — the **task lens**: relevance-ranked *signatures* plus doc-comments
  and cx/in metrics, framed for reuse ("compose from these"). Use it when the question is "what should I
  build on / touch for THIS task".
- Rule of thumb: `--for` for a task you're about to do; `--query` for a vocabulary you're hunting. Both shine
  on specific technical wording; for broad common-word asks, plain `rg` + one read can still win.

**Budgeting trap:** a positive, explicit `--top-k` is inert on plain `--for`. Narrow plain `--for` with its own arguments: `--signatures-only` (no auto-bodies), `--token-budget=N` (shapes
the bundle to fit), `--detail=N` (full bodies for just the top N). `--top-k` does shape `--query`. `--expand`/`--outline` drop the map by default; pass `--top-k=N` only if you want the map back in the same call.

## When the fixed verbs can't phrase the question

Compose filters over the call graph with `--graph-query=EXPR` — see **ripwire-rs-graph-query** for the
mini-language (sources · kind/cx/fanin/file filters · bounded callers/callees closure · and/or/not joins).

<example title="Tracing blast radius before refactoring">
Good: Run `ripwire-rs <dir> --impact=Interner` to see all 25 transitive callers across the repository before renaming a method.
Bad: Grep for 'Interner' and assume only local file usages exist.
</example>
</instructions>

<output_format>
### Navigation Trace
- **Target Symbol**: [Symbol name and file:line]
- **Direct Callers**: [1-hop callers and signatures]
- **Outgoing Callees**: [Key functions invoked]
- **Transitive Blast Radius**: [Symbol count and impacted files]
- **Safety Assessment**: [Local vs architectural blast radius]
</output_format>
