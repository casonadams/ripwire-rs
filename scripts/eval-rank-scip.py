#!/usr/bin/env python3
"""Grade `--rank-by=authority|hub` against a compiler SCIP index.

Ranks REPO with one or more binaries, takes each top N, and scores every
symbol by the edges the SCIP overlay pins: `--callers` for authority (what
relies on it), `--callees` for hub (what it reaches). Edges come from the
judging binary run with `--scip=INDEX`, so neither ranking grades itself by
its own name resolution.

Metrics per binary:
  edges>=5   share with five or more compiler-bound edges
  0-edges    share with none
  files      median distinct other files on the far side of those edges
  >=3-files  share reaching three or more other files
  local      share whose edges never leave the symbol's own file

Build INDEX first (rust-analyzer scip, scip-typescript, scip-python).

Usage:
  scripts/eval-rank-scip.py REPO --scip INDEX --judge PATH
                            [--bin ours=PATH] [--bin cpp=PATH]
                            [--rank authority|hub] [--top 25]
"""

import argparse
import re
import statistics
import subprocess
import sys
import xml.etree.ElementTree as ET


def ranked(binary, repo, rank, top):
    out = subprocess.run(
        [binary, ".", f"--rank-by={rank}"], capture_output=True, text=True, cwd=repo
    ).stdout
    root = ET.fromstring(out)
    rows = [
        (f.get("p"), s.get("n"), s.get("sc"), float(s.get("k") or 0))
        for f in root.iter("f")
        for s in f.iter("s")
    ]
    return sorted(rows, key=lambda r: -r[3])[:top]


def selector(path, name, scope):
    return "::".join(part for part in (path, scope, name) if part)


def edges(judge, repo, scip, verb, sym):
    out = subprocess.run(
        [judge, ".", f"--scip={scip}", f"--{verb}={sym}", "--legend=compact", "--limit=500"],
        capture_output=True,
        text=True,
        cwd=repo,
    ).stdout
    count = re.search(rf'<{verb} [^>]*count="(\d+)"', out)
    if not count:
        return None
    body = out.split("-->", 1)[-1]
    files = {p.split(":")[0] for p in re.findall(r' p="([^"]+)"', body)}
    return int(count.group(1)), files


def grade(rows, judge, repo, scip, verb):
    counts, spread = [], []
    for path, name, scope, _ in rows:
        found = edges(judge, repo, scip, verb, selector(path, name, scope))
        if found is None:
            continue
        counts.append(found[0])
        spread.append(len(found[1] - {path}))
    if not counts:
        return None
    share = lambda xs, pred: sum(pred(x) for x in xs) / len(xs)
    return {
        "graded": len(counts),
        "edges>=5": share(counts, lambda c: c >= 5),
        "0-edges": share(counts, lambda c: c == 0),
        "files": statistics.median(spread),
        ">=3-files": share(spread, lambda f: f >= 3),
        "local": share(spread, lambda f: f == 0),
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("repo")
    parser.add_argument("--scip", required=True, help="SCIP index of REPO")
    parser.add_argument("--judge", required=True, help="ripwire-rs binary that reads --scip")
    parser.add_argument("--bin", action="append", default=[], help="LABEL=PATH to rank with")
    parser.add_argument("--rank", choices=("authority", "hub"), default="authority")
    parser.add_argument("--top", type=int, default=25)
    args = parser.parse_args()
    bins = [b.split("=", 1) for b in args.bin] or [["ours", args.judge]]
    verb = "callers" if args.rank == "authority" else "callees"
    for label, binary in bins:
        result = grade(ranked(binary, args.repo, args.rank, args.top), args.judge, args.repo, args.scip, verb)
        if result is None:
            print(f"{label:10} no symbol resolved", file=sys.stderr)
            continue
        print(
            f"{label:10} graded={result['graded']:3} edges>=5={result['edges>=5']:4.0%} "
            f"0-edges={result['0-edges']:4.0%} files={result['files']:4.1f} "
            f">=3-files={result['>=3-files']:4.0%} local={result['local']:4.0%}"
        )


if __name__ == "__main__":
    main()
