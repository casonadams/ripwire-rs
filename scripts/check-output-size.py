#!/usr/bin/env python3
"""Fail when a default answer grows past its byte budget, or states a token price that is not true.

Reads scripts/output-budgets.toml, runs each command against the repository with the given binary
and compares stdout bytes to `max_bytes`. A budget is a ceiling for the default answer on this
repository, not a target: raise one only with a reason in the commit that raises it.

Answers that disclose `est_tokens=` are also tokenized (o200k_base, via tiktoken) and the disclosed
price must be within TOKEN_TOLERANCE of the real count. The estimate is bytes / BYTES_PER_TOKEN; the
constant is calibrated from these measurements, so a drift in the output's density shows up here.
Skipped, loudly, when tiktoken is not importable (`uv run --with tiktoken scripts/check-output-size.py`).

Usage: scripts/check-output-size.py [REPO] [--bin PATH]
"""

import argparse
import pathlib
import re
import shlex
import subprocess
import sys
import tomllib

BUDGETS = pathlib.Path(__file__).with_name("output-budgets.toml")
# est_tokens / real tokens must fall in this band; measured 0.87 to 1.08 across map, for and pack-task.
TOKEN_TOLERANCE = (0.80, 1.20)


def run(binary, repo, command):
    return subprocess.run(
        [binary, repo, *shlex.split(command)], capture_output=True, timeout=120
    ).stdout


def tokenizer():
    try:
        import tiktoken

        return tiktoken.get_encoding("o200k_base")
    except Exception:  # missing module, or no cached vocabulary and no network
        return None


def price_failure(command, out, encoder):
    """A message when the disclosed est_tokens is not within tolerance of the real count, else None."""
    found = re.search(rb'est_tokens="?(\d+)', out)
    if not found or encoder is None:
        return None
    real = len(encoder.encode(out.decode(errors="replace"), disallowed_special=()))
    ratio = int(found[1]) / max(real, 1)
    low, high = TOKEN_TOLERANCE
    ok = low <= ratio <= high
    print(f"{'ok  ' if ok else 'FAIL'} est_tokens={found[1].decode()} real={real} ratio={ratio:.2f} {command or '(default map)'}")
    return None if ok else f"{command or '(default map)'} est_tokens ratio {ratio:.2f}"


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("repo", nargs="?", default=".")
    parser.add_argument("--bin", default="target/release/ripwire-rs")
    args = parser.parse_args()
    budgets = tomllib.loads(BUDGETS.read_text())["budget"]
    encoder = tokenizer()
    if encoder is None:
        print("skip est_tokens accuracy: tiktoken unavailable; run with `uv run --with tiktoken`")
    failures = []
    for entry in budgets:
        command, limit = entry["command"], entry["max_bytes"]
        out = run(args.bin, args.repo, command)
        mark = "ok  " if len(out) <= limit else "OVER"
        print(f"{mark} {len(out):>7} / {limit:<7} {command or '(default map)'}")
        if len(out) > limit:
            failures.append(command or "(default map)")
        if message := price_failure(command, out, encoder):
            failures.append(message)
    if failures:
        print(f"::error::answer size or token price wrong: {', '.join(failures)}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main())
