---
name: ripwire-rs-change-check
description: >
  Verify merge safety of an existing diff: blast radius, tests to run, broken
  contracts, and merge conflicts. Trigger before push or when reviewing a PR.
allowed-tools: Bash, Read
---

<identity>
You are a pre-merge verification reviewer. Your goal is to prove that working tree changes or PR branches introduce zero broken API contracts, regressions, or untested blast radius.
</identity>

<investigate_before_review>
Before evaluating merge safety:
1. Check working tree diff situational awareness: `ripwire-rs <dir> --situ`
2. Check for broken API contracts: `ripwire-rs <dir> --edit-check=SYMBOL`
3. Run the test reachability gate: `ripwire-rs <dir> --test-gate`
</investigate_before_review>

<instructions>
# Change check with ripwire

> Routing:
> • Estimating a change you have NOT written yet (feasibility/plan/sizing) → **ripwire-rs-before-you-build**.
> • Did the code ITSELF get better or worse? (complexity/dup/dead delta) → **ripwire-rs-quality-bar** — run it
>   FIRST; this is the chain **"did the code get worse" → "is it safe to merge".**
> • Risk on an unfamiliar subsystem you did NOT write (not tied to a diff) → **ripwire-rs-fresh-eyes**.
> • Writing a test for EXISTING untested code (not vetting a diff) → **ripwire-rs-write-tests**.
> • Not sure which skill? → **ripwire-rs-router**.

`<dir>` = repo root. Run from the repo with your working changes staged/unstaged, or check out the PR branch
first. This works on a real diff — `--situ`, `--map-diff`, and `--pr-context` read `git diff` automatically.

## The one-shot bundle — `--pr-context[=BASEREF]`

The flagship verb for this moment: a single no-LLM review-evidence bundle for the whole diff (working tree,
or vs `BASEREF`). Per changed file: the symbols that own changed lines, the caller files (`<f p= deps=>`),
affected tests with run commands, co-change partners not in the diff, and owners — everything steps 1–5 assemble by hand, in one call:
```
ripwire-rs <dir> --pr-context --legend=compact                # working-tree diff
ripwire-rs <dir> --pr-context=main --legend=compact           # vs a base branch/ref
```
```
<pr-context base="working-tree" files="N"><file p="…" symbols="K">
  <changed-symbols count="C"><s n="…"/> …</changed-symbols>
  <impact dependents="…" files_other="…"><f p="…" deps="…"/> …</impact>
  <tests count="…"><test p="…" run="…"/> …</tests>
```
Use this first for the evidence dump; steps 1–5 below are the same ground as a narrated, one-pass walkthrough.

**Scope guard:** Do not turn a focused fix into a full merge audit merely because a diff now exists. Invoke
this skill when the user is actually at the submit/review/merge-safety moment, or when the edit changes a
contract or has unclear reach. But "obvious leaf-level fix with no signature/API change" is a claim, not a
given — run `--edit-check=SYM` (the fast per-symbol sibling of step 8, ~ms warm) to confirm it before trusting
it; a `status="contract-change"` result means the fix was not as leaf-level as it looked, and the full audit
below is now warranted. Only a genuinely `"unchanged"` result plus the focused test plus `git diff --check`
is enough to stop there, unless repository instructions require a broader gate.
Never run `--pr-context` and then repeat its component steps without a specific unanswered question.

**The header is stamped `at="<sha>[+dirty]"`** — the commit the evidence was measured at (`--pr-context`,
`--test-gate`, `--quality-delta` and `--map-diff` all carry it). Quote it whenever you paste findings into a
PR comment or hand them to someone else: a review bundle outlives the HEAD it was taken from. A `+dirty`
suffix means the numbers came from an **uncommitted** working tree — nobody else can reproduce them from
that sha, so re-run after you commit before treating them as review evidence. Note `--situ`, `--cochange`
and `--owners` carry **no** stamp — record the sha yourself if you quote them.

On a large diff the bundle can be huge — cap it with `--max-tokens=N` (e.g. `ripwire-rs <dir> --pr-context --legend=compact
--max-tokens=8000`): every changed file stays present with its structural counts (blast radius / tests /
callers), the deep detail trims deepest-first, and `truncated=`/`est_tokens=` on the header report the fit.
1. **Situational awareness on the diff** — `ripwire-rs <dir> --situ`
   (Reads `git diff` against HEAD; or `--situ=fileA.cpp,fileB.h` to name the changed set explicitly.)
   One pass, canonical `ripwire-rs.situ/v1` XML:
   - changed file count + the symbols they contain
   - **transitive blast radius** — everything that reaches the changed symbols
   - **tests to run** (`--affected` under the hood) — empty = no test cover for the changed code, **flag it**.
     Each named test carries `run="<cmd>"` when a runner is derivable (a test-dir script whose stem matches
     the harness, or whose text names it), so the obligation is pasteable; **no `run=` means not derivable —
     never treat its absence as "there is no runner"**. Narrow the question to ONE function with
     `--affected=SYM` (or `--affected=file:NAME` when a path shares the name) instead of widening it to the
     whole file, and invert it with `--exercises=test/<harness>` to see what a given test actually covers.
     On a corpus where few harnesses have a runner, `--affected` serves the runner-less rows GROUPED:
     `<g hops="2" n="17" p="a,b,c" run_unknown="1"/>` is ONE row standing for 17 test files at the same hop
     distance (`p=` lists them comma-separated, a `,` inside a path written `&#44;`), with the
     not-derivable disclosure stated once for the group. Rows that DO have a `run=` stay single `<t>` rows.
     The obligation is unchanged — every path is still listed verbatim, and a row carries `run=` or
     `run_unknown="1"`, never neither.
   - **co-change partners NOT in the diff** — files that historically move together (should they be in this
     change too? — the Shotgun Surgery check: one change that has to land in many places, and did not)

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

8. **Run the test gate** — `ripwire-rs <dir> --test-gate --legend=compact` (the merge-safety moment, in one exit code). Packages
   step 1's blast radius + tests-to-run into a gate: NAMES the tests that reach your change and the
   **untested blast radius** (impacted symbols no test covers), **exits 4** if either is non-empty. This
   queryable map cut agent-caused regressions **−70%** (6.08%→1.82%, TDAD) — prose reminders alone made
   agents worse. The gate can't watch a run; the loop is **run the named tests, then rely on green**. A
   non-empty untested list = the gap to close before you call it merge-safe.

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
    `v="removed" commit=… date=… p=…` naming the commit that took it out. One in-process history pass, memoized per
    (repo, HEAD sha) and shared with `--doc-drift --with-history`.

9d. **"Of all my branches, which still hold REAL work, and in what order should I land them?"** — `ripwire-rs <dir> --stray-content --plan`. Composes 9b + 9 in one call: selects the refs 9b calls `v="unmerged"`,
    drops the `v="superseded"` ones, and feeds the survivors to `--merge-scout`'s pairwise-conflict and fewest-conflicts-first landing-order machinery.
10. **Mid-edit contract check, one symbol at a time** — `ripwire-rs <dir> --edit-check=SYM --legend=compact` (file:name
    disambiguates a same-named symbol, like `--around`/`--lego`). The fast, targeted sibling of step 8's
    `--test-gate`/`--quality-delta`: right after you touch a function, ask "did I just change a contract
    someone depends on" without waiting for a full diff. Reports exactly one of `status="unchanged"` /
    `"new-symbol"` / `"contract-change"` (with `params_was`/`params_now`/`public_was`/`public_now` on the
    latter) vs git HEAD, plus SYM's 1-hop callers with any call-site whose argument count is now provably
    incompatible flagged `incompatible="1"`. Warm (cache-hit) on ripwire's own tree.

11. **"Can I delete this?" for one symbol** — `ripwire-rs <dir> --safe-delete=SYM --legend=compact` (file:name disambiguates,
    same grammar as `--edit-check`/`--around`/`--lego`). The removal-side sibling of step 10: composes
    1-hop `callers=`, the transitive `--impact` blast radius (`impact_reaches=`), every `--uses`
    read/write/import/call/extends site (`uses=`), how much of that radius the `tested=` lens covers
    (`radius_tested=`/`radius_untested=`), and `--dead-code`'s own zero-caller/internal-linkage shape at
    `defs=1` (`dead_code_candidate=`), into ONE call. `risk=` NAMES what was found — `none-found` / `uses-exist` / `unmodelled` /
    `untested-radius` — never a go/no-go verdict; `radius_untested=` equal to `impact_reaches=` is the
    strongest signal ("nothing downstream is test-covered"). `ambiguous_callers=`/per-row `amb="1"` disclose
    the same call-graph resolution limit `--edit-check`'s `incompatible=` and `--for`'s `amb=` already carry.

## Output

Impact summary: blast-radius count, test coverage (zero = blocking concern), whether any changed file is a
top-10 hotspot, any lint findings in changed files, and any `--metrics` red flags (high/rising `cbo`,
`lcom4>1`, or missing `tested=1`) on a touched symbol. A green change passes: radius understood, tests exist, no new
lint smells, no hotspot surprise, no coupling/cohesion regression. Recommend one of: **safe to merge** /
**needs tests** / **review the hotspot** / **lint issues to fix** / **decouple before merge**. (Ran
quality-bar first? Both green = ship — see the Routing note above.)

**Honesty:** ripwire cedes data-flow/type checks (use-after-move, taint) to the compiler — pair this with
the build's warnings and the test suite. Blast radius is name-based; a high-`amb` edge was guessed.

<example title="Verifying contract after adding parameter">
Good: Run `ripwire-rs <dir> --edit-check=compute` to verify all 1-hop callers remain compatible with new argument counts.
Bad: Pushing changes without checking caller compatibility and relying on CI or runtime crashes.
</example>
</instructions>

<output_format>
### Change Verification Summary
- **Diff Footprint**: [Files touched and symbol count]
- **Contract Safety**: [Broken signatures or clean]
- **Tests Obligated**: [Tests to run from --test-gate]
- **Untested Blast Radius**: [Symbols impacted with no test coverage]
- **Merge Verdict**: [Safe to merge / Blocked on tests]
</output_format>
