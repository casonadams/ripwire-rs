#!/usr/bin/env python3
"""Score call resolution against compiler SCIP indexes of pinned public repositories, locally.

scripts/corpus/Dockerfile holds every indexer in one linux/amd64 image. corpus.toml pins each repository to
a full commit SHA, its indexer command and the precision and recall floors measured on it. Checkouts,
indexes and logs live under ~/.cache/ripwire-corpus; nothing is written into this repository.

Usage:
  corpus.py index [NAME...] [--force] [--strict]
  corpus.py eval [NAME...] [--bin PATH] [--strict]
  corpus.py measure [NAME...] [--bin PATH] [--strict]
  corpus.py diff OLD_BIN NEW_BIN [NAME...] [--samples N] [--strict]
  corpus.py status [NAME...]
"""

import argparse
import dataclasses
import hashlib
import math
import os
import pathlib
import re
import shutil
import subprocess
import sys
import tomllib

ROOT = pathlib.Path(__file__).resolve().parents[2]
MANIFEST = pathlib.Path(__file__).with_name("corpus.toml")
CACHE = pathlib.Path.home() / ".cache" / "ripwire-corpus"
IMAGE = "ripwire-corpus:local"
DEFAULT_BIN = ROOT / "target" / "release" / "ripwire-rs"
TIMEOUT = 3600
MARGIN = 0.002
EVAL_TAG = re.compile(r"<scip-eval ([^>]*)>")
SITE_TAG = re.compile(r"<site ([^>]*)/>")
ATTR = re.compile(r'([\w-]+)="([^"]*)"')
SAMPLE_SHOWN = 5


class Skip(Exception):
    """An entry that cannot run here. Reported loudly; never counted as a floor miss."""


@dataclasses.dataclass(frozen=True)
class Entry:
    name: str
    languages: tuple
    status: str
    license: str
    url: str
    sha: str
    path: str
    indexer: str
    indexer_version: str
    index_file: str
    setup: str
    index: str
    reason: str
    precision_floor: float | None
    recall_floor: float | None

    @classmethod
    def from_table(cls, table):
        return cls(
            name=table["name"],
            languages=tuple(table.get("languages", ())),
            status=table.get("status", "unverified"),
            license=table.get("license", ""),
            url=table.get("url", ""),
            sha=table.get("sha", ""),
            path=table.get("path", ""),
            indexer=table["indexer"],
            indexer_version=table.get("indexer_version", ""),
            index_file=table.get("index_file", "index.scip"),
            setup=table.get("setup", ""),
            index=table.get("index", ""),
            reason=table.get("reason", ""),
            precision_floor=table.get("precision_floor"),
            recall_floor=table.get("recall_floor"),
        )


@dataclasses.dataclass(frozen=True)
class Score:
    entry: Entry
    calls: int
    covered: int
    precision: float | None
    recall: float | None

    def misses(self):
        """Metrics that fell below their floor; a missing measurement counts as a miss."""
        out = []
        if self.entry.precision_floor is not None and (
            self.precision is None or self.precision < self.entry.precision_floor
        ):
            out.append("precision")
        if self.entry.recall_floor is not None and (
            self.recall is None or self.recall < self.entry.recall_floor
        ):
            out.append("recall")
        return out


def select(names):
    """Manifest entries in file order, restricted to NAMES when given."""
    tables = tomllib.loads(MANIFEST.read_text())["repo"]
    entries = [Entry.from_table(table) for table in tables]
    unknown = sorted(set(names) - {entry.name for entry in entries})
    if unknown:
        sys.exit(f"corpus: unknown entries: {', '.join(unknown)}")
    return [entry for entry in entries if not names or entry.name in names]


def must(args, what, **kwargs):
    done = subprocess.run(args, capture_output=True, text=True, **kwargs)
    if done.returncode:
        raise Skip(f"{what} failed: {done.stderr.strip()[-300:]}")
    return done.stdout


def checkout(entry):
    """Clone once, then move the working tree to the pinned SHA."""
    dest = CACHE / "src" / entry.name
    if not (dest / ".git").is_dir():
        dest.mkdir(parents=True, exist_ok=True)
        must(["git", "init", "-q"], "git init", cwd=dest)
        must(["git", "remote", "add", "origin", entry.url], "git remote add", cwd=dest)
    head = subprocess.run(
        ["git", "rev-parse", "-q", "--verify", "HEAD"], cwd=dest, capture_output=True, text=True
    ).stdout.strip()
    if head != entry.sha:
        must(["git", "fetch", "-q", "--depth", "1", "origin", entry.sha], f"fetch {entry.sha[:12]}", cwd=dest)
        must(["git", "checkout", "-q", "-f", "--detach", "FETCH_HEAD"], "git checkout", cwd=dest)
        must(["git", "clean", "-q", "-ffdx"], "git clean", cwd=dest)
    return dest


def worktree_paths(src):
    listing = must(["git", "ls-files", "-z", "-co", "--exclude-standard"], "git ls-files", cwd=src)
    return [rel for rel in listing.split("\0") if rel]


def copy_into(source, target):
    if source.is_dir():  # a symlinked directory, such as vendor/grammars
        shutil.copytree(source, target, dirs_exist_ok=True)
    elif source.is_file():
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)


def tree_digest(root):
    digest = hashlib.sha256()
    for path in sorted(p for p in root.rglob("*") if p.is_file()):
        digest.update(str(path.relative_to(root)).encode())
        digest.update(path.read_bytes())
    return digest.hexdigest()


def snapshot(entry):
    """Copy the local working tree (tracked and unignored files) so the index matches what is on disk."""
    src = ROOT / entry.path
    dest = CACHE / "src" / entry.name
    shutil.rmtree(dest, ignore_errors=True)
    dest.mkdir(parents=True)
    for rel in worktree_paths(src):
        copy_into(src / rel, dest / rel)
    return dest


def docker_ready():
    if shutil.which("docker") is None:
        raise Skip("docker is not installed")
    if subprocess.run(["docker", "info", "--format", "{{.ServerVersion}}"], capture_output=True).returncode:
        raise Skip("docker daemon is not running")
    if subprocess.run(["docker", "image", "inspect", IMAGE], capture_output=True).returncode:
        raise Skip(f"image {IMAGE} is not built: run make corpus-image")


def container_script(entry):
    index = entry.index.replace("{out}", f"/out/{entry.index_file}")
    return "\n".join(line for line in (entry.setup, index) if line)


def docker_command(entry, src, out):
    return [
        "docker", "run", "--rm", "--platform", "linux/amd64",
        "--user", f"{os.getuid()}:{os.getgid()}",
        "-v", f"{src}:/src", "-v", f"{out}:/out", "-v", f"{CACHE / 'home'}:/corpus-home",
        "-w", "/src",
        "-e", "HOME=/corpus-home",
        "-e", "CARGO_HOME=/corpus-home/cargo",
        "-e", "COMPOSER_HOME=/corpus-home/composer",
        "-e", "RUSTUP_TOOLCHAIN=1.98.1",
        "-e", "GIT_CONFIG_COUNT=1", "-e", "GIT_CONFIG_KEY_0=safe.directory", "-e", "GIT_CONFIG_VALUE_0=*",
        IMAGE, "sh", "-ec", container_script(entry),
    ]


def build_index(entry, src, target):
    out = CACHE / "out" / entry.name
    shutil.rmtree(out, ignore_errors=True)
    out.mkdir(parents=True)
    (CACHE / "home").mkdir(parents=True, exist_ok=True)
    log = CACHE / "logs" / f"{entry.name}.log"
    log.parent.mkdir(parents=True, exist_ok=True)
    with log.open("w") as handle:
        done = subprocess.run(
            docker_command(entry, src, out), stdout=handle, stderr=subprocess.STDOUT, timeout=TIMEOUT
        )
    produced = out / entry.index_file
    if done.returncode or not produced.exists():
        raise Skip(f"indexer {entry.indexer} failed (exit {done.returncode}, log {log})")
    target.parent.mkdir(parents=True, exist_ok=True)
    shutil.move(produced, target)


def prepare(entry, force):
    """The checkout (or snapshot) and its SCIP index, built when missing or when FORCE."""
    if entry.path:
        src = snapshot(entry)
        key = tree_digest(src)[:12]
    else:
        src = checkout(entry)
        key = entry.sha[:12]
    index = CACHE / "index" / f"{entry.name}-{key}-{entry.indexer_version}.scip"
    if force or not index.exists():
        docker_ready()
        build_index(entry, src, index)
    return src, index


def find_binary(path):
    if path:
        if not os.access(path, os.X_OK):
            sys.exit(f"corpus: {path} is not an executable ripwire-rs binary")
        return path
    if DEFAULT_BIN.exists():
        return str(DEFAULT_BIN)
    found = shutil.which("ripwire-rs")
    if found is None:
        sys.exit("corpus: no ripwire-rs binary: run make build-release or pass --bin")
    return found


def scip_eval(binary, src, index, samples):
    """Run `ripwire-rs SRC --scip INDEX --eval=scip`; return (raw output, attributes of <scip-eval>)."""
    args = [binary, str(src), "--scip", str(index), "--eval=scip", f"--samples={samples}"]
    done = subprocess.run(args, capture_output=True, text=True, timeout=TIMEOUT)
    found = EVAL_TAG.search(done.stdout)
    if done.returncode or not found:
        raise Skip(f"{binary} --eval=scip failed: {done.stderr.strip()[-300:]}")
    return done.stdout, dict(ATTR.findall(found[1]))


def ratio(attrs, key):
    return float(attrs[key]) if key in attrs else None


def score_entry(entry, binary):
    src, index = prepare(entry, force=False)
    _, attrs = scip_eval(binary, src, index, 0)
    return Score(entry, int(attrs.get("calls", 0)), int(attrs.get("covered", 0)),
                 ratio(attrs, "precision"), ratio(attrs, "recall"))


def suggested(value):
    """A floor just under MEASURED, rounded down to four places, keeping MARGIN of headroom."""
    return math.floor(round((value - MARGIN) * 10000, 6)) / 10000


def fmt(value):
    return "-" if value is None else f"{value:.4f}"


def print_table(scores):
    print(f"{'name':<12} {'languages':<16} {'calls':>7} {'covered':>7} {'precision':>9} {'floor':>7} "
          f"{'recall':>7} {'floor':>7}  result")
    for score in scores:
        entry = score.entry
        result = "ok" if not score.misses() else "FAIL " + ",".join(score.misses())
        print(f"{entry.name:<12} {','.join(entry.languages):<16} {score.calls:>7} {score.covered:>7} "
              f"{fmt(score.precision):>9} {fmt(entry.precision_floor):>7} {fmt(score.recall):>7} "
              f"{fmt(entry.recall_floor):>7}  {result}")


def run_entries(entries, action):
    """Run ACTION on each entry; return the names skipped (blocked in the manifest, or unable to run)."""
    skipped = []
    for entry in entries:
        if entry.status == "blocked":
            print(f"SKIP {entry.name}: blocked: {entry.reason}", file=sys.stderr)
            skipped.append(entry.name)
            continue
        try:
            action(entry)
        except Skip as exc:
            print(f"SKIP {entry.name}: {exc}", file=sys.stderr)
            skipped.append(entry.name)
    return skipped


def cmd_index(args):
    def action(entry):
        _, index = prepare(entry, args.force)
        print(f"{entry.name}: {index}")

    skipped = run_entries(select(args.names), action)
    return 1 if args.strict and skipped else 0


def cmd_eval(args):
    binary = find_binary(args.bin)
    scores = []
    skipped = run_entries(select(args.names), lambda entry: scores.append(score_entry(entry, binary)))
    print_table(scores)
    misses = [score.entry.name for score in scores if score.misses()]
    if misses:
        print(f"::error::corpus floors missed: {', '.join(misses)}", file=sys.stderr)
    return 1 if misses or (args.strict and skipped) else 0


def cmd_measure(args):
    binary = find_binary(args.bin)
    scores = []
    skipped = run_entries(select(args.names), lambda entry: scores.append(score_entry(entry, binary)))
    print_table(scores)
    print("\nsuggested floors (measured minus %.3f, rounded down):" % MARGIN)
    for score in scores:
        p = "-" if score.precision is None else f"{suggested(score.precision):.4f}"
        r = "-" if score.recall is None else f"{suggested(score.recall):.4f}"
        print(f"  {score.entry.name}: precision_floor = {p}  recall_floor = {r}")
    return 1 if args.strict and skipped else 0


def parse_sites(text):
    """Disagreeing call sites keyed by (file:line, callee); agreeing sites are not printed by --eval=scip."""
    sites = {}
    for tag in SITE_TAG.findall(text):
        attrs = dict(ATTR.findall(tag))
        sites.setdefault((attrs.get("p", ""), attrs.get("callee", "")), attrs)
    return sites


def classify(old, new):
    """Sites that entered, left, or changed in the disagreement list between two binaries."""
    entered = sorted(key for key in new if key not in old)
    left = sorted(key for key in old if key not in new)
    changed = sorted(
        key for key in new
        if key in old and (new[key].get("v"), new[key].get("ours")) != (old[key].get("v"), old[key].get("ours"))
    )
    return entered, left, changed


def report_diff(name, old_attrs, new_attrs, old_sites, new_sites):
    entered, left, changed = classify(old_sites, new_sites)
    print(f"{name}: calls {old_attrs.get('calls')} -> {new_attrs.get('calls')}, "
          f"precision {old_attrs.get('precision')} -> {new_attrs.get('precision')}, "
          f"recall {old_attrs.get('recall')} -> {new_attrs.get('recall')}")
    print(f"  entered disagreement (right->wrong or new): {len(entered)}")
    print(f"  left disagreement (wrong->right or dropped): {len(left)}")
    print(f"  same site, different verdict or edge: {len(changed)}")
    for label, keys, sites in (("entered", entered, new_sites), ("left", left, old_sites), ("changed", changed, new_sites)):
        for key in keys[:SAMPLE_SHOWN]:
            print(f"    {label}: {key[0]} callee={key[1]} v={sites[key].get('v')} ours={sites[key].get('ours', '')}")
    return len(entered), len(left), len(changed)


def cmd_diff(args):
    old_bin, new_bin = find_binary(args.old), find_binary(args.new)
    totals = [0, 0, 0]

    def action(entry):
        src, index = prepare(entry, force=False)
        old_text, old_attrs = scip_eval(old_bin, src, index, args.samples)
        new_text, new_attrs = scip_eval(new_bin, src, index, args.samples)
        counts = report_diff(entry.name, old_attrs, new_attrs, parse_sites(old_text), parse_sites(new_text))
        totals[:] = [a + b for a, b in zip(totals, counts)]

    skipped = run_entries(select(args.names), action)
    print(f"total: entered {totals[0]}, left {totals[1]}, changed {totals[2]}")
    return 1 if args.strict and skipped else 0


def cmd_status(args):
    docker = "available" if shutil.which("docker") else "not installed"
    image = "built" if subprocess.run(["docker", "image", "inspect", IMAGE], capture_output=True).returncode == 0 else "not built"
    print(f"docker: {docker}; image {IMAGE}: {image}")
    for entry in select(args.names):
        cached = sorted((CACHE / "index").glob(f"{entry.name}-*.scip")) if (CACHE / "index").exists() else []
        index = cached[-1].name if cached else "no index"
        extra = f" blocked: {entry.reason}" if entry.status == "blocked" else ""
        print(f"{entry.name:<12} {entry.status:<11} {entry.indexer} {entry.indexer_version:<8} {index}{extra}")
    return 0


def build_parser():
    parser = argparse.ArgumentParser(description="Corpus precision and recall against SCIP indexes.")
    sub = parser.add_subparsers(dest="cmd", required=True)
    for name in ("index", "eval", "measure", "status"):
        child = sub.add_parser(name)
        child.add_argument("names", nargs="*")
        child.add_argument("--strict", action="store_true", help="fail on skipped entries too")
        child.add_argument("--force", action="store_true", help="rebuild the index even when cached")
        child.add_argument("--bin", help="ripwire-rs binary (default target/release/ripwire-rs)")
    diff = sub.add_parser("diff")
    diff.add_argument("old")
    diff.add_argument("new")
    diff.add_argument("names", nargs="*")
    diff.add_argument("--samples", type=int, default=100000)
    diff.add_argument("--strict", action="store_true")
    return parser


HANDLERS = {
    "index": cmd_index,
    "eval": cmd_eval,
    "measure": cmd_measure,
    "diff": cmd_diff,
    "status": cmd_status,
}


def main(argv=None):
    args = build_parser().parse_args(argv)
    return HANDLERS[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
