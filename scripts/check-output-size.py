#!/usr/bin/env python3
"""Fail when a default answer grows past its byte budget.

Reads scripts/output-budgets.toml, runs each command against the repository with the given binary
and compares stdout bytes to `max_bytes`. A budget is a ceiling for the default answer on this
repository, not a target: raise one only with a reason in the commit that raises it.

Usage: scripts/check-output-size.py [REPO] [--bin PATH]
"""

import argparse
import pathlib
import shlex
import subprocess
import sys
import tomllib

BUDGETS = pathlib.Path(__file__).with_name("output-budgets.toml")


def measure(binary, repo, command):
    proc = subprocess.run(
        [binary, repo, *shlex.split(command)], capture_output=True, timeout=120
    )
    return len(proc.stdout)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("repo", nargs="?", default=".")
    parser.add_argument("--bin", default="target/release/ripwire-rs")
    args = parser.parse_args()
    budgets = tomllib.loads(BUDGETS.read_text())["budget"]
    failures = []
    for entry in budgets:
        command, limit = entry["command"], entry["max_bytes"]
        size = measure(args.bin, args.repo, command)
        mark = "ok " if size <= limit else "OVER"
        print(f"{mark} {size:>7} / {limit:<7} {command or '(default map)'}")
        if size > limit:
            failures.append(command or "(default map)")
    if failures:
        print(f"::error::default answer over budget: {', '.join(failures)}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
