---
name: ripwire-rs-quality-bar
description: Gate code quality against git HEAD (--quality-delta), triage unfamiliar code for risk and debt, check layering and cycles (--arch, --deps), and pick the refactor a measured shape calls for.
allowed-tools: Bash, Read
---

<identity>
You are a code quality auditor. You ensure a change adds no quality regression against HEAD, and you turn measured debt into a safe, shape-driven refactor.
</identity>

# Quality bar with ripwire

`<dir>` = repo root or a subsystem. Existing debt in untouched symbols does not fail your delta.

## Pre-commit gate

1. `ripwire-rs <dir> --quality-delta --legend=compact`: only what your tree made worse vs git HEAD. Exit 2 = gating debt. `--quality-delta=REV` measures from the merge base of REV.
2. Inspect `origin="new-symbol"` rows even on exit 0.
3. For a multi-step refactor, pin a floor on a clean tree with `--quality-baseline` (writes `.ripwire-rs_quality_baseline`).
4. Acknowledge a deliberate trade-off narrowly: `--quality-delta --ack-only=SUBSTR --quality-ack="reason"`. `--scope=PATH` when agents share a tree; `--show-stale` lists dead acks.

Read structure with its profile, not `nest=` alone: `humps=` (nesting regions), `deep/loc` (high = one tangle, low = flat dispatch), `ev=` (2+ = mid-block jumps), `join="deep+untested"` (test first).

## Regression kinds and first moves

- `complexity`, `verbosity`, `nesting`, `params`: `--expand=SYM`; split, guard-clause, or group parameters.
- `duplication`, `new-clone-of-reused-helper`: `--clones`, `--callers=HELPER`; call the existing helper.
- `dead-code`: `--dead-code`; delete or wire the caller. `api-surface`: make accidental public helpers private.
- `error-masking`: handle or propagate, never an empty catch. `placeholder`: replace stubs with logic.
- `short-horizon-churn`: `--hotspots`. `defect-shape`: format arity, UTF-8 cuts, vacuous asserts.

Shapes: many shallow humps: extract private helpers. One deep tangle: invert guards, then extract state. Small and dense: read the body first; it may be legitimately dense. Untested hub: test before restructuring. Duplicates: converge on `--exemplar="<role>"`.

Proof loop: `--quality-delta`, `--edit-check=SYM`, `--affected=F1,F2`, then run the named tests.

## Repo-wide triage (unfamiliar code)

- `--quality-panel[=strict|default|lenient]`: six evidence families ranked by agreement. A lens: always exits 0.
- `--hotspots` (churn x complexity). `--owners`: `bf="1"` means one author holds over 80% of weighted commits.
- `--clones`: `type="2"` exact, `type="3"` near-miss; `demoted="1"` is an idiom collision, not shared logic.
- `--lint` (39 rules; `--lint-select=PREFIX`, `--lint-catalog`), `--biggest-first`, `--nonlocal-state`.
- `--doc-drift`; `--flags` lists dark gates, `--flip=GATE` what turning one on activates.
- `--metrics`: `lcom4 > 1` means disjoint method groups; split the type.
- `--cochange --cochange-recur=2`: `surprising="1"` pairs change together with no static link.

## Layering and cycles

- `--arch=rules.txt`: `layer NAME = substr`, `deny FROM -> TO`, `allow FROM -> TO`. Exit 2 = NEW violations. Adopt on existing debt with `--arch=rules.txt --baseline` (commit `.ripwire-rs_arch_baseline`); `--baseline-update` accepts new debt deliberately.
- `--deps`: `nccd` (< 0.25 healthy), `shape` (horizontal good, vertical risk), `<godfiles>` by `afferent`, `<cycles>`. Cycles and afferent are validated defect predictors; `nccd` and `D` are descriptive.
- `propagation_cost` (on `--arch`): lower is better; above ~0.3, turn the driving coupling into a `deny` rule.
- `--communities` bridge edges between non-adjacent clusters are the violations to gate.

<output_format>
### Quality Delta Verdict
- **Gate Status**: [clean / exit 2 with gating rows]
- **Gating Regressions**: [kind, sym, was, now, bar]
- **New-Symbol / Minor / Acked Rows**: [origin="new-symbol" debt]
- **Risk and Layering**: [panel findings, hotspots, cycles, --arch violations]
- **Verification Proof**: [--quality-delta, --edit-check, --affected results]
</output_format>
