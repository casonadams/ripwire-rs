# ripwire-rs-change-check: reference

Steps 2-7 and 9-9d of the change-check walkthrough, moved out of `SKILL.md`.

9. **Landing several concurrent branches?** — `ripwire-rs <dir> --merge-scout=REF1,REF2,... --legend=compact` (read-only, in-process git). For each REF it takes the merge-base with HEAD and the functions, methods and types whose lines the REF changed since then (`<sym>` rows). Per pair it reports **conflicts** (both refs changed the same symbol: a real merge will fight over it) and **risks** (files both changed with no shared symbol: still worth a glance). `head_conflicts=` counts an arm's symbols that HEAD also changed since that base; `<no-work/>` marks an arm with nothing divergent; `<landing order="…"/>` is the fewest-conflicts-first sequence to land the arms with work. An unknown REF is an error. Run this BEFORE picking a merge order for several agent branches instead of hand-diffing each pair.

9b. **Is anything STRANDED on a branch — and was it already re-done?** — `ripwire-rs <dir> --stray-content --legend=compact`
    (`=SUBSTR` filters ref names). `--merge-scout` above answers "which of these named branches collide";
    this answers the prior question — *of all my branches, which still hold work the live line does not
    have?* Per ref it reports the lines that ref's own work AUTHORED (vs its merge-base with HEAD) that HEAD
    lacks, with a verdict: `unmerged` (genuinely absent — this is the queue to work), `superseded` (the live
    line removed the SAME base code this ref removed, i.e. it re-implemented the work), `merged` (omitted).
    **`superseded` is the case `git cherry` structurally cannot see** — it compares commit ancestry, so a fix
    the live line re-did differently stays "unmerged" forever, which is exactly how a finished fix sits on a
    branch for days behind a ledger that says "ported". Every row prints its raw `del=`/`redone=`/`sim=`
    evidence — read those before acting, the verdict is a summary, not an oracle. Line-granular, not
    semantic. Read-only, single-root.

9c. **"Where does this content live?"** — `ripwire-rs <dir> --whereis=SYM --legend=compact`. Which ref's tree defines or mentions
    a symbol, HEAD first; `on-head="0"` alongside branch hits is content that exists ONLY on a branch. Each
    distinct blob is read once (git is content-addressed), so 30 branches cost about one tree. `kind="def"`
    is a parser's answer on every row — HEAD's from the index, a branch row's from the same extraction over that
    branch's blob (only blobs holding a line a loose lexical test calls worth parsing are parsed; `kind="text"` is
    such a line no parse read, never a definition). By default only the `kind="def"` rows are listed and the mentions are
    counted in one `<refs count=N next=…>` row (its `next=` lists them) when that page lists MORE definitions than
    the all page does under the same row cap (a capped all page can list fewer), or the same ones in strictly fewer
    bytes; otherwise, and with `--whereis-listing=all`, every row is listed. A line over 300 bytes prints its `t=` as a
    200-byte preview (`t_bytes=` is the whole line's size) when that makes the page shorter; the page's `<t_cut next=…>` replays
    it with `--whereis-full-text`, every line whole.
    A tree scan only finds what some ref *still carries*, so `hits="0"` cannot by itself tell a name this repo
    never had from one it deleted. Add `--with-history` and a `<fate>` row says which: `v="never"`, or
    `v="removed" commit=… date=… p=…` naming the commit that took it out, or `v="unavailable"` with a `note=` when
    the history could not be read, so neither claim is made. One in-process history pass, memoized per
    (repo, HEAD sha) and shared with `--doc-drift --with-history`.

9d. **"Of all my branches, which still hold REAL work, and in what order should I land them?"** — `ripwire-rs <dir> --stray-content --plan`. Composes 9b + 9 in one call: selects the refs 9b calls `v="unmerged"`,
    drops the `v="superseded"` ones, and feeds the survivors to `--merge-scout`'s pairwise-conflict and fewest-conflicts-first landing-order machinery.
2. **Hotspot risk** — `ripwire-rs <dir> --hotspots --legend=compact`
   `<hotspots>` ranked by `score = churn × ccx`. Does any changed file appear in the top-10? A change that
   touches a high-score file deserves extra scrutiny; one that *raises* ccx in an already-churny file is a
   regression risk.

3. **Lint delta** — `ripwire-rs <dir> --lint --legend=compact`
   `<lint findings="N">` per-rule summary, then per-finding file + enclosing symbol. Cross against the
   changed-file list from step 1 — **any finding in a touched file is one this change introduced or inherited.**

4. **Missing-test seam check** — if step 1 showed `tests="0"`, run `ripwire-rs <dir> --seams --legend=compact` to see whether the
   changed code crosses an integration seam with no test coverage. That's the gap to fill before merging.

5. **The diff's structural footprint** — `ripwire-rs <dir> --map-diff --legend=compact`
   Emits ONLY the symbols changed vs git HEAD, ranked — the change's footprint in one screen, without the
   rest of the map as noise. Use `--rank-by=ccx` to prioritize cognitive complexity;
   use `--hotspots` separately for git change-frequency risk. Supported map signals
   are pagerank, authority, hub, degree, ccx, and stable; unknown signals error.

6. **Read the numbers on what you touched** — `ripwire-rs <dir> --metrics --legend=compact` (also carried inline by `--for`/
   `--around --metrics`). Cross these against your changed set:
   | Attr | Means | Threshold → action |
   |---|---|---|
   | `cbo` | distinct dependencies (coupling between objects) | high (≳8, or *rose* vs neighbours) = **fragile, hard to test in isolation** → decouple: hide a dependency behind an interface or invert it |
   | `lcom4` | class cohesion = # connected method components (class-kinds only) | `>1` = the class does **N unrelated jobs** (N disjoint method clusters) → split it into N types |
   | `nest` | max block-nesting depth | `>~4`, or above the file's local median → deep-branch smell → guard clauses / extract the inner block into a named fn |
   | `loc` | lines in the symbol | well above the local median for its kind → size smell → split; a giant new fn is the #1 agent verbosity failure mode |
   | `params` | parameter count | `>~5` → bundle related params into a struct rather than widening the signature |
   | `tested` | is any indexed test reaching it | `tested="1"` = a safety net is present; omission on this explicit metrics surface means no indexed test reaches it → add one before changing it further |
   These are size-correlated signals (coupling is the validated one; complexity/size are heuristics) — read
   the delta against the file's own median, not an absolute bar. A number that was already high before your
   diff is not your regression (that judgment is **ripwire-rs-quality-bar**'s `--quality-delta`).

7. **Docs-sync check** — `ripwire-rs <dir> --mentions=SYM --legend=compact` for each changed symbol from step 1. Any markdown doc
   that backtick-names a symbol you just changed is a staleness candidate — the design rationale it wrote
   down may no longer match the code. Skim the listed docs; update or flag the ones that describe behavior
   your diff altered.

