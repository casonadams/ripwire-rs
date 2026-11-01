# ripwire-rs-orient: reference

Sections moved out of `SKILL.md`: orienting N agents at once, zooming out on big repos, field notes, and resuming after a compaction.

## Leave a note for next time — the gotcha you just learned (field notes)

The most expensive thing you rebuild across sessions is **gotchas, not structure**. When you learn a
non-obvious fact about a symbol or file (a race trap, an off-by-one seam, "don't touch this without
re-running X"), pin it so the *next* orientation surfaces it automatically:

```bash
ripwire-rs . --note-add="Bar::compute: recompute is NOT idempotent — reset the arena first"
ripwire-rs . --note-add="src/pool.h: 128-byte cache line on Apple, never hardcode 64"   # a file also works
```

The note is free text: `--note-add` appends the line you give it to `.ripwire-rs_notes` verbatim. Lead with the
symbol or path it is about (`SYM: text`), by the name you would search for; ripwire does not resolve, canonicalise or
validate the target. Notes live in committed `.ripwire-rs_notes`, and `ripwire-rs . --notes` lists every note. They do
not attach to `--for`/`--expand` output, so read `--notes` yourself when you resume work. `--recall=TASK` is the
doc-level complement.
`--note-add` nudges (stderr, non-blocking) toward writing the decision, not a description — a note that
keeps firing on the same symbol has outgrown a comment: graduate it into a `--quality-ack` reason or a
standing `--arch` deny rule.

## Resuming — a compaction, or a new session on work already in flight

A different moment from a cold start: you are **not** cold on the repo, you are cold on **your own last
hour**. The task is known; what evaporated is the reasoning, the gotchas already paid for, and what you had
half-changed. Re-reading source rebuilds the *least* valuable of those. Run the three verbs that rebuild the
rest, in this order:

```bash
ripwire-rs . --recall="<the task, in the words you'd use>"   # 1. what past sessions WROTE DOWN
ripwire-rs . --situ                                          # 2. what the working tree already CHANGED
ripwire-rs . --notes                                         # 3. gotchas already paid for
```

1. **`--recall`** returns the *full bodies* of the most relevant markdown only — memory notes,
   planning/design docs, READMEs — so code can't swamp them. This is the decisions-and-rationale layer that a
   compaction destroys and that source code never contained in the first place. Point it at your memory dir
   for past-session memory, or the repo root for the project's plans.
2. **`--situ`** (defaults to `git diff`) tells you what you had already changed, its blast radius, the tests
   to run, and the co-change partners you hadn't touched yet — i.e. where you actually stopped, and what you
   were about to break. This is the step that most often reveals work-in-flight you would otherwise redo.
3. **`--notes`** lists every pinned gotcha (free-text lines). Read it for anything about the symbols you are about to
   touch; no other report surfaces them.

Then, and only then, escalate the ladder for whatever is still missing. Two honest cautions: `--recall`
returns what the docs **claim**, not what is still true — a stale plan doc reads exactly as confidently as a
current one, so trust `--situ`'s working-tree facts over a doc when they disagree. And `--situ` carries **no
`at=` commit stamp**, so if you are resuming across a rebase or a moved HEAD, record `git rev-parse --short
HEAD` yourself before you quote anything from it.

**Before the next compaction, spend the note.** The compaction you are recovering from is the argument for
`--note-add`: a gotcha written to `.ripwire-rs_notes` survives a context reset; one held only in context does
not. When you are deep in a task and learn something non-obvious, pin it *then* — not at the end.

## Orienting N agents at once, not yourself — `--partition=N`

About to fan a single task out to several parallel agents? Do **not** let each one run its own
`--pack-task` — they will each re-derive the same top symbols, the same bodies, the same tests, and you
pay for the map N times. Run it **once**:

```bash
ripwire-rs <dir> --pack-task="<the task in words>" --legend=compact --partition=4
```

You get one `<ctx-partitions>` document: a **shared common core** (the anchors the task is literally about,
what every agent needs) plus **N per-agent slices** carved along the call graph's own Leiden communities,
so a slice is a union of whole modules rather than an arbitrary rank cut (symbols the call graph is silent
about — edgeless data types — group by *file* instead, so one header's structs stay together). Each `<bundle>` wraps a complete,
standalone bundle — hand one bundle to one agent verbatim. `--token-budget` here means **one agent's**
budget (core + its slice), not the document's; `--json` gives the same plan machine-readably. MCP: the same
thing as a `partition` argument on the `explore` verb.

**Read the wrapper attributes before you trust the split** — the verb reports its own quality:
`overlap_max` (worst pairwise Jaccard between slices; low = the agents really are reading different code),
`split="K"` (K>0 means there were fewer modules than agents, so a module was cut at its rank median — the
slices are less semantically clean), `partitions` < `requested` (the task's surface could not supply N
separable slices at all — take fewer agents), and `core_overlap` (how much of the core a slice reaches
anyway). On a task whose whole surface sits inside one module, a partition is a rank cut, not a module
boundary — one `--pack-task` and one agent is the honest answer there.

## When a flat module list is too coarse (big repos) — zoom out

**7. Nested module hierarchy** — `ripwire-rs <dir> --zoom --legend=compact` (`--zoom=DEPTH` to cap levels; the default prints the top 2
levels of the 40 largest modules — `levels_shown=`/`shown=` disclose it, `next=` pastes the next page, `--zoom-levels=0`
prints every level): multi-level Leiden,
`<module level=N id= size= dir=>`, indent = one level deeper, innermost `level="0"` lists top-ranked members.
Read top-down; a `dir=` that doesn't match its parent's is a cross-cutting concern in the wrong place.
Trailing `<bridge …>` entries name the high-traffic integration seams *between* top modules — pair with
`--seams` to see which ones no test reaches.

**8. Render it** — `ripwire-rs <dir> --zoom --mermaid` (or `--mermaid` for the flat module graph): a
`flowchart TB`, paste at mermaid.live. For hand-exploring, `ripwire-rs <dir> --html[=FILE]` writes a
self-contained clickable wiki (module cards → subgraphs → Sourcetrail-style node recentering, no CDN);
`--color-by=community|cx|churn|tested` sets its initial node-colour lens (a live selector covers the rest).
