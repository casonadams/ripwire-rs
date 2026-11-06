#!/usr/bin/env python3
"""Score call resolution against compiler SCIP indexes of pinned public repositories, locally.

scripts/corpus/Dockerfile holds every indexer in one linux/amd64 image; an entry with platform = linux/arm64 runs in its own
arm64 image. corpus.toml pins each repository to
a full commit SHA, its indexer command and the precision and recall floors measured on it. Checkouts,
indexes and logs live under ~/.cache/ripwire-corpus; nothing is written into this repository.

Usage:
  corpus.py index [NAME...] [--force] [--strict]
  corpus.py eval [NAME...] [--bin PATH] [--strict]
  corpus.py measure [NAME...] [--bin PATH] [--strict]
  corpus.py diff OLD_BIN NEW_BIN [NAME...] [--samples N] [--strict]
  corpus.py status [NAME...]
  corpus.py verify [run|ratchet|changed|watch|status|warm] [NAME...] [--bin PATH] [--lang L] [--diff[=OLD_BIN]]
  corpus.py add URL --language LANG [--name N] [--sha SHA] [--path SUBDIR] [--measure] [--dry-run]   (see add.py)
  corpus.py add --list-languages
  corpus.py remove NAME [--yes]
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
DEFAULT_PLATFORM = "linux/amd64"
PLATFORMS = (DEFAULT_PLATFORM, "linux/arm64")
DEFAULT_BIN = ROOT / "target" / "release" / "ripwire-rs"
TIMEOUT = 3600
MARGIN = 0.002
EVAL_TAG = re.compile(r"<scip-eval ([^>]*)>")
VERDICT_TAG = re.compile(r"<verdict ([^>]*)/>")
# Verdicts scored as right or wrong; unindexed sites are neither (see scip_eval.rs SiteVerdict).
STATE = {
    "exact": "right", "split-hit": "right", "external-agreed": "right",
    "wrong": "wrong", "missed": "wrong", "false-edge": "wrong",
    "unindexed": "neutral",
}
RANK = {"neutral": 0, "right": 1, "wrong": 2}
FLIPS = ("right->wrong", "wrong->right", "entered", "left")
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
    subdir: str
    indexer: str
    indexer_version: str
    index_file: str
    setup: str
    index: str
    image: str
    platform: str
    reason: str
    precision_floor: float | None
    recall_floor: float | None

    @classmethod
    def from_table(cls, table):
        platform = table.get("platform", DEFAULT_PLATFORM)
        if platform not in PLATFORMS:
            raise ValueError(f"{table['name']}: platform {platform!r} must be one of {', '.join(PLATFORMS)}")
        return cls(
            name=table["name"],
            languages=tuple(table.get("languages", ())),
            status=table.get("status", "unverified"),
            license=table.get("license", ""),
            url=table.get("url", ""),
            sha=table.get("sha", ""),
            path=table.get("path", ""),
            subdir=table.get("subdir", ""),
            indexer=table["indexer"],
            indexer_version=table.get("indexer_version", ""),
            index_file=table.get("index_file", "index.scip"),
            setup=table.get("setup", ""),
            index=table.get("index", ""),
            image=table.get("image", IMAGE),
            platform=platform,
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


def docker_ready(entry):
    if shutil.which("docker") is None:
        raise Skip("docker is not installed")
    if subprocess.run(["docker", "info", "--format", "{{.ServerVersion}}"], capture_output=True).returncode:
        raise Skip("docker daemon is not running")
    inspect = subprocess.run(
        ["docker", "image", "inspect", "--format", "{{.Architecture}}", entry.image], capture_output=True, text=True
    )
    if inspect.returncode:
        raise Skip(f"image {entry.image} is not built: run the make target that builds it (see the corpus-image* targets)")
    arch = inspect.stdout.strip()
    if arch != entry.platform.partition("/")[2]:
        raise Skip(f"image {entry.image} is {arch}, but {entry.name} runs on {entry.platform}: build it for that platform")


def container_script(entry):
    index = entry.index.replace("{out}", f"/out/{entry.index_file}")
    return "\n".join(line for line in (entry.setup, index) if line)


def docker_command(entry, src, out):
    capped = ["--memory=3g"] if entry.platform == DEFAULT_PLATFORM else []
    return [
        "docker", "run", "--rm", "--platform", entry.platform, *capped,
        "--user", f"{os.getuid()}:{os.getgid()}",
        "-v", f"{src}:/src", "-v", f"{out}:/out", "-v", f"{CACHE / 'home'}:/corpus-home",
        "-w", "/src",
        "-e", "HOME=/corpus-home",
        "-e", "CARGO_HOME=/corpus-home/cargo",
        "-e", "COMPOSER_HOME=/corpus-home/composer",
        "-e", "RUSTUP_TOOLCHAIN=1.98.1",
        "-e", "GIT_CONFIG_COUNT=1", "-e", "GIT_CONFIG_KEY_0=safe.directory", "-e", "GIT_CONFIG_VALUE_0=*",
        entry.image, "sh", "-ec", container_script(entry),
    ]


def read_varint(buf, pos):
    """The unsigned base-128 varint at POS, and the position just after it."""
    end = pos
    while buf[end] & 0x80:
        end += 1
    value = 0
    for index, byte in enumerate(buf[pos:end + 1]):
        value |= (byte & 0x7F) << (7 * index)
    return value, end + 1


def wire_fields(buf):
    """Each top-level protobuf field as (number, wire type, its raw bytes, its payload when length-delimited)."""
    pos = 0
    while pos < len(buf):
        start = pos
        key, pos = read_varint(buf, pos)
        number, wire = key >> 3, key & 7
        payload = None
        if wire == 0:
            _, pos = read_varint(buf, pos)
        elif wire == 1:
            pos += 8
        elif wire == 2:
            length, pos = read_varint(buf, pos)
            payload = buf[pos:pos + length]
            pos += length
        elif wire == 5:
            pos += 4
        else:
            raise ValueError(f"unsupported protobuf wire type {wire}")
        yield number, wire, buf[start:pos], payload


def scip_document_path(document):
    for number, wire, _raw, payload in wire_fields(document):
        if number == 1 and wire == 2:
            return payload.decode()
    return ""


def scip_subdir(index_bytes, subdir):
    """The SCIP index with only the documents under SUBDIR kept; every other top-level field is copied byte for byte."""
    prefix = subdir.strip("/") + "/"
    kept = []
    for number, _wire, raw, payload in wire_fields(index_bytes):
        if number != 2 or scip_document_path(payload).startswith(prefix):
            kept.append(raw)
    return b"".join(kept)


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
    if entry.subdir:
        target.write_bytes(scip_subdir(produced.read_bytes(), entry.subdir))
    else:
        shutil.move(produced, target)


def index_path(entry, key):
    """Where the index of ENTRY at KEY (a SHA prefix or a tree digest prefix) is cached."""
    return CACHE / "index" / f"{entry.name}-{key}-{entry.indexer_version}.scip"


def prepare(entry, force):
    """The checkout (or snapshot) and its SCIP index, built when missing or when FORCE."""
    if entry.path:
        src = snapshot(entry)
        key = tree_digest(src)[:12]
    else:
        src = checkout(entry)
        key = entry.sha[:12]
    index = index_path(entry, key)
    if force or not index.exists():
        docker_ready(entry)
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


def run_eval(args, binary):
    """Run an eval command; return (raw output, attributes of <scip-eval>)."""
    done = subprocess.run(args, capture_output=True, text=True, timeout=TIMEOUT)
    found = EVAL_TAG.search(done.stdout)
    if done.returncode or not found:
        raise Skip(f"{binary} --eval=scip failed: {done.stderr.strip()[-300:]}")
    return done.stdout, dict(ATTR.findall(found[1]))


def scip_eval(binary, src, index, samples):
    """Score a build against an index: (raw output, attributes of <scip-eval>)."""
    args = [binary, str(src), "--scip", str(index), "--eval=scip", f"--samples={samples}"]
    return run_eval(args, binary)


def scip_verdicts(binary, src, index, samples=0):
    """Score a build and list every covered site's verdict (--eval-sites); SAMPLES names disagreeing sites too."""
    args = [binary, str(src), "--scip", str(index), "--eval=scip", f"--samples={samples}", "--eval-sites"]
    return run_eval(args, binary)


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


def parse_states(text):
    """Each covered call site's state, keyed by (file:line, callee): right, wrong or neutral (unindexed).

    --eval-sites prints one <verdict> per covered reference. A key with several references takes the
    worst of them: one wrong reference makes the site wrong.
    """
    states = {}
    for tag in VERDICT_TAG.findall(text):
        attrs = dict(ATTR.findall(tag))
        key = (attrs.get("p", ""), attrs.get("callee", ""))
        state = STATE[attrs["v"]]
        states[key] = max(states.get(key, "neutral"), state, key=RANK.get)
    return states


def flips(old, new):
    """Sites by flip between two builds. A site missing from a build is 'absent' there.

    right->wrong is a regression, wrong->right a fix, entered a site that now disagrees but was not wrong
    before, left a wrong site that no longer disagrees. Right->neutral and neutral->right are not reported.
    """
    out = {name: [] for name in FLIPS}
    for key in sorted(set(old) | set(new)):
        before, after = old.get(key, "absent"), new.get(key, "absent")
        if (before, after) == ("right", "wrong"):
            out["right->wrong"].append(key)
        elif (before, after) == ("wrong", "right"):
            out["wrong->right"].append(key)
        elif after == "wrong" and before != "wrong":
            out["entered"].append(key)
        elif before == "wrong" and after != "wrong":
            out["left"].append(key)
    return out


def report_diff(name, old_attrs, new_attrs, old_states, new_states, samples):
    result = flips(old_states, new_states)
    print(f"{name}: calls {old_attrs.get('calls')} -> {new_attrs.get('calls')}, "
          f"precision {old_attrs.get('precision')} -> {new_attrs.get('precision')}, "
          f"recall {old_attrs.get('recall')} -> {new_attrs.get('recall')}")
    for label in FLIPS:
        print(f"  {label}: {len(result[label])}")
        for key in result[label][:samples]:
            print(f"    {label}: {key[0]} callee={key[1]}")
    return {label: len(result[label]) for label in FLIPS}


def cmd_diff(args):
    old_bin, new_bin = find_binary(args.old), find_binary(args.new)
    totals = dict.fromkeys(FLIPS, 0)

    def action(entry):
        src, index = prepare(entry, force=False)
        old_text, old_attrs = scip_verdicts(old_bin, src, index)
        new_text, new_attrs = scip_verdicts(new_bin, src, index)
        counts = report_diff(entry.name, old_attrs, new_attrs,
                             parse_states(old_text), parse_states(new_text), args.samples)
        for label, count in counts.items():
            totals[label] += count

    skipped = run_entries(select(args.names), action)
    print("total: " + ", ".join(f"{label} {totals[label]}" for label in FLIPS))
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
    diff.add_argument("--samples", type=int, default=SAMPLE_SHOWN,
                      help="sites printed per flip (the counts are always exact)")
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
    argv = sys.argv[1:] if argv is None else list(argv)
    if argv[:1] == ["verify"]:
        import verify  # imports this module back; the verification commands build on its helpers

        return verify.main(argv[1:])
    if argv[:1] and argv[0] in ("add", "remove"):
        import add  # the repository lifecycle; add.py builds on this module's helpers

        return add.main(argv)
    args = build_parser().parse_args(argv)
    return HANDLERS[args.cmd](args)


if __name__ == "__main__":
    sys.exit(main())
