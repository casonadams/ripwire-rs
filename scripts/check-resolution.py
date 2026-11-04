#!/usr/bin/env python3
"""Fail when call resolution on this repository gets worse.

Every verb inherits the call graph, so the resolver's gauges are the quality ceiling. Two readings:

* The gauges on the root of any navigation answer: `graph_ambiguous` (calls bound to one of several
  same-named definitions) and `graph_unresolved` (calls that bound to nothing). They count what the
  resolver gave up on.
* A SCIP index from rust-analyzer, the compiler's own answer, scored with `--eval=scip --scip=INDEX`: `precision` (a
  binding we made names SCIP's target) and `recall` (SCIP's targets we reached). They say whether the
  bindings we made are right. Skipped, loudly, when rust-analyzer is not installed.

scripts/resolution-ceilings.toml holds a ceiling for each gauge and a floor for each SCIP metric on this
repository; a change that crosses one fails, and a change that improves one a lot should move the bound
in the same commit so the gain stays locked in.

Usage: scripts/check-resolution.py [REPO] [--bin PATH]
"""

import argparse
import pathlib
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib

BOUNDS = pathlib.Path(__file__).with_name("resolution-ceilings.toml")
PROBE = "--callers=fit_rows"


def run(args, **kwargs):
    return subprocess.run(args, capture_output=True, text=True, timeout=300, **kwargs)


def gauges(binary, repo):
    out = run([binary, repo, PROBE]).stdout
    return {key: int(value) for key, value in re.findall(r'(graph_\w+)="(\d+)"', out)}


def scip_scores(binary, repo):
    """precision and recall against rust-analyzer's SCIP index, or None when it cannot be built."""
    if shutil.which("rust-analyzer") is None:
        return None
    with tempfile.TemporaryDirectory() as tmp:
        index = pathlib.Path(tmp) / "index.scip"
        built = run(["rust-analyzer", "scip", repo, "--output", str(index)])
        if built.returncode != 0 or not index.exists():
            print(built.stderr[-400:], file=sys.stderr)
            return None
        out = run([binary, repo, "--scip", str(index), "--eval=scip", "--samples=0"]).stdout
    found = re.search(r'<scip-eval [^>]*precision="([\d.]+)" recall="([\d.]+)"', out)
    return {"precision": float(found[1]), "recall": float(found[2])} if found else None


def check(measured, key, limit, over):
    value = measured.get(key)
    ok = value is not None and not over(value, limit)
    print(f"{'ok  ' if ok else 'FAIL'} {key}={value} (bound {limit})")
    return ok


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("repo", nargs="?", default=".")
    parser.add_argument("--bin", default="target/release/ripwire-rs")
    args = parser.parse_args()
    bounds = tomllib.loads(BOUNDS.read_text())
    measured = gauges(args.bin, args.repo)
    failures = [
        entry["gauge"]
        for entry in bounds["ceiling"]
        if not check(measured, entry["gauge"], entry["max"], lambda v, m: v > m)
    ]
    scores = scip_scores(args.bin, args.repo)
    if scores is None:
        print("skip SCIP precision/recall: rust-analyzer unavailable or failed; install it to run this gate")
    else:
        failures += [
            entry["metric"]
            for entry in bounds["floor"]
            if not check(scores, entry["metric"], entry["min"], lambda v, m: v < m)
        ]
    if failures:
        print(f"::error::call resolution worse than its bound: {', '.join(failures)}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
