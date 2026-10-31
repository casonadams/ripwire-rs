#!/usr/bin/env python3
"""Fail when call resolution on this repository gets worse.

Every verb inherits the call graph, so the resolver's gauges are the quality ceiling. The root of any
navigation answer carries them: `graph_ambiguous` (calls bound to one of several same-named definitions)
and `graph_unresolved` (calls that bound to nothing). scripts/resolution-ceilings.toml holds a ceiling
for each on this repository; a change that raises either past its ceiling fails, and a change that
lowers one a lot should lower the ceiling in the same commit so the gain stays locked in.

Usage: scripts/check-resolution.py [REPO] [--bin PATH]
"""

import argparse
import pathlib
import re
import subprocess
import sys
import tomllib

CEILINGS = pathlib.Path(__file__).with_name("resolution-ceilings.toml")
PROBE = "--callers=fit_rows"


def gauges(binary, repo):
    out = subprocess.run(
        [binary, repo, PROBE], capture_output=True, text=True, timeout=120
    ).stdout
    return {key: int(value) for key, value in re.findall(r'(graph_\w+)="(\d+)"', out)}


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("repo", nargs="?", default=".")
    parser.add_argument("--bin", default="target/release/ripwire-rs")
    args = parser.parse_args()
    ceilings = tomllib.loads(CEILINGS.read_text())["ceiling"]
    measured = gauges(args.bin, args.repo)
    failures = []
    for entry in ceilings:
        key, limit = entry["gauge"], entry["max"]
        value = measured.get(key)
        ok = value is not None and value <= limit
        print(f"{'ok  ' if ok else 'OVER'} {key}={value} (ceiling {limit})")
        if not ok:
            failures.append(key)
    if failures:
        print(f"::error::call resolution worse than its ceiling: {', '.join(failures)}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
