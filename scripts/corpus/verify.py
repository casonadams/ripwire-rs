"""Local verification of the resolver corpus: one scoreboard, a ratchet, changed-language runs and diffs.

Reached as `corpus.py verify [MODE] [NAME...] [--bin PATH] [--lang L] [--diff[=OLD_BIN]]`; the Makefile wraps
each mode (verify, verify-ratchet, verify-changed, verify-diff, verify-watch, verify-status, verify-warm).

  run      score every entry with a cached index and the Rust self-check; compare with baseline.toml (default)
  ratchet  run, then record the measured numbers in baseline.toml and raise the corpus.toml floors to the
           measured value minus 0.002; refuses, and writes nothing, when a number dropped or an entry failed
  changed  run the entries whose languages the working tree's changes touch. A change under src/ outside
           src/languages/<lang>/, a Cargo file or corpus.toml touches every entry; tests/languages/<lang>*.rs
           touches that language. LANG and NAME narrow it further.
  watch    rebuild the fast binary when src/ changes and print only the entries whose numbers moved
  status   stale or missing indexes, Docker and image state, cache size
  warm     build the missing indexes, sequentially (Docker)

--diff adds, after run or changed, the sites whose verdict flipped right->wrong between OLD_BIN and the binary
under test (default OLD_BIN: ~/.cache/ripwire-corpus/bin/baseline, written by make verify-keep-bin).
"""

import argparse
import concurrent.futures
import dataclasses
import hashlib
import itertools
import os
import pathlib
import re
import shutil
import subprocess
import sys
import time
import tomllib

import corpus

ROOT = corpus.ROOT
CACHE = corpus.CACHE
BASELINE = pathlib.Path(__file__).with_name("baseline.toml")
BASELINE_BIN = CACHE / "bin" / "baseline"
CHECK_RESOLUTION = ROOT / "scripts" / "check-resolution.py"
NOISE = 0.0005
METRICS = ("precision", "recall")
MODES = ("run", "ratchet", "changed", "watch", "status", "warm")
ALL = None  # a scope that covers every entry
WHOLE_TREE = frozenset({"Cargo.toml", "Cargo.lock", "scripts/corpus/corpus.toml"})
SELF = "ripwire-rs-self"
SELF_PIN = "check-resolution"
SCORE_LINE = re.compile(r"^(?:ok|FAIL)\s+(precision|recall)=(\S+) \(bound", re.M)
SITE_TAG = re.compile(r"<site ([^>]*)>")
SITE_SAMPLES = 100000  # every disagreeing site, so the diff can name ours and scip for each flip
WATCH_POLL = 1.0
NAME_LINE = re.compile(r'^name = "([^"]*)"')
FLOOR_LINE = re.compile(r"^(precision_floor|recall_floor) = ")


@dataclasses.dataclass
class Row:
    """A measured entry (or the self-check): its numbers, or why it was not measured, or what is wrong with it."""

    entry: corpus.Entry
    pin: str = ""
    calls: int | None = None
    covered: int | None = None
    precision: float | None = None
    recall: float | None = None
    skip: str = ""
    problem: str = ""


@dataclasses.dataclass
class Verdict:
    row: Row
    status: str  # ok | NEW | FAIL | SKIP
    reasons: tuple = ()
    base: dict | None = None


@dataclasses.dataclass
class Diff:
    name: str
    lines: list
    counts: dict
    skip: str = ""


def self_entry():
    return corpus.Entry.from_table({"name": SELF, "languages": ["rust"], "indexer": "rust-analyzer"})


def selected(args):
    """Manifest entries named by NAME (positional) and LANG, in manifest order."""
    entries = corpus.select(args.names)
    if args.lang:
        entries = [entry for entry in entries if args.lang in entry.languages]
    return entries


def known_languages():
    return {lang for entry in corpus.select([]) for lang in entry.languages}


# Pins and indexes

def pin_of(entry):
    """The tree to score and the pin naming its index: the commit SHA, or the digest of a local tree's snapshot."""
    if entry.path:
        src = corpus.snapshot(entry)
        return src, f"tree:{corpus.tree_digest(src)}"
    return corpus.checkout(entry), entry.sha


def index_key(pin):
    return pin.removeprefix("tree:")[:12]


def locate(entry):
    """The tree, its pin and its cached index. Never builds an index: Skip when none is cached."""
    src, pin = pin_of(entry)
    index = corpus.index_path(entry, index_key(pin))
    if not index.exists():
        raise corpus.Skip(f"no cached index {index.name}: run make verify-warm")
    return src, index, pin


def comparable(base, row):
    """A record is comparable only while its pin and indexer match the entry (a re-pin resets the numbers)."""
    return base is not None and base["sha"] == row.pin and base["indexer_version"] == row.entry.indexer_version


# Measuring (worker processes)

def measure(entry, binary):
    """Score one entry on its cached index. An entry that cannot be measured comes back as a skipped Row."""
    if entry.status == "blocked":
        return Row(entry, skip=f"blocked: {entry.reason}")
    try:
        src, index, pin = locate(entry)
        _, attrs = corpus.scip_eval(binary, src, index, 0)
    except corpus.Skip as exc:
        return Row(entry, skip=str(exc))
    return Row(entry, pin, int(attrs.get("calls", 0)), int(attrs.get("covered", 0)),
               corpus.ratio(attrs, "precision"), corpus.ratio(attrs, "recall"))


def parse_self(text):
    """(precision, recall, skip reason) from check-resolution.py's output; a None value when SCIP was not scored."""
    if "skip SCIP" in text:
        return None, None, "rust-analyzer unavailable or failed (make resolution explains)"
    found = {key: (None if value == "None" else float(value)) for key, value in SCORE_LINE.findall(text)}
    return found.get("precision"), found.get("recall"), ""


def measure_self(binary):
    """The Rust self-check: check-resolution.py's SCIP precision and recall on this tree; a failing gauge is a problem."""
    done = subprocess.run([sys.executable, str(CHECK_RESOLUTION), str(ROOT), "--bin", binary],
                          capture_output=True, text=True)
    precision, recall, skip = parse_self(done.stdout)
    problem = "" if done.returncode == 0 else f"make resolution fails (exit {done.returncode})"
    return Row(self_entry(), SELF_PIN, precision=precision, recall=recall, skip=skip, problem=problem)


def measure_all(entries, binary, with_self):
    """Every entry's Row, then the self-check row when WITH_SELF; measured across worker processes, capped at the CPUs."""
    jobs = len(entries) + int(with_self)
    if not jobs:
        return []
    with concurrent.futures.ProcessPoolExecutor(max_workers=min(os.cpu_count() or 1, jobs)) as pool:
        futures = [pool.submit(measure, entry, binary) for entry in entries]
        if with_self:
            futures.append(pool.submit(measure_self, binary))
        return [future.result() for future in futures]


# Judging against the baseline and the floors

def recorded(base, metric):
    return None if base is None else base[metric]


def metric_problems(metric, value, floor, recorded_value):
    out = []
    if floor is not None and (value is None or value < floor):
        out.append(f"{metric} {corpus.fmt(value)} below floor {floor:.4f}")
    if recorded_value is not None and (value is None or value < recorded_value - NOISE):
        out.append(f"{metric} {corpus.fmt(value)} dropped from {recorded_value:.4f}")
    return out


def judge(row, base):
    """The verdict for ROW against its recorded BASE (None when there is none): a floor miss or a drop beyond NOISE fails."""
    if row.problem:
        return Verdict(row, "FAIL", (row.problem,), base)
    if row.skip:
        return Verdict(row, "SKIP", (row.skip,), base)
    reasons = []
    for metric in METRICS:
        reasons += metric_problems(metric, getattr(row, metric), getattr(row.entry, f"{metric}_floor"),
                                   recorded(base, metric))
    if reasons:
        return Verdict(row, "FAIL", tuple(reasons), base)
    return Verdict(row, "NEW" if base is None else "ok", (), base)


def run_scoreboard(entries, binary, with_self):
    baseline = load_baseline()
    verdicts = []
    for row in measure_all(entries, binary, with_self):
        base = baseline.get(row.entry.name)
        verdicts.append(judge(row, base if comparable(base, row) else None))
    return verdicts


def raisable(verdict):
    base, row = verdict.base, verdict.row
    return verdict.status == "ok" and base is not None and any(
        getattr(row, metric) is not None and getattr(row, metric) - base[metric] > NOISE for metric in METRICS
    )


# Output

def change(value, recorded_value):
    return "-" if value is None or recorded_value is None else f"{value - recorded_value:+.4f}"


def result_text(verdict):
    return verdict.status if not verdict.reasons else f"{verdict.status}: {'; '.join(verdict.reasons)}"


def table_row(verdict):
    row, base = verdict.row, verdict.base
    head = f"{row.entry.name:<16}{','.join(row.entry.languages):<14}"
    if verdict.status == "SKIP":
        return f"{head}  SKIP: {verdict.reasons[0]}"
    p, r = row.precision, row.recall
    bp, br = recorded(base, "precision"), recorded(base, "recall")
    calls = "-" if row.calls is None else str(row.calls)
    covered = "-" if row.covered is None else str(row.covered)
    return (f"{head}{calls:>7}{covered:>9}{corpus.fmt(p):>11}{corpus.fmt(bp):>8}{change(p, bp):>9}"
            f"{corpus.fmt(r):>8}{corpus.fmt(br):>8}{change(r, br):>9}  {result_text(verdict)}")


def format_table(verdicts):
    """The scoreboard: each entry's precision and recall against its baseline, the change, and the verdict per entry."""
    header = (f"{'name':<16}{'languages':<14}{'calls':>7}{'covered':>9}{'precision':>11}{'base':>8}{'delta':>9}"
              f"{'recall':>8}{'base':>8}{'delta':>9}  result")
    return "\n".join([header, *(table_row(verdict) for verdict in verdicts)])


def verdict_line(verdicts):
    failed = [verdict for verdict in verdicts if verdict.status == "FAIL"]
    if failed:
        return "VERIFY FAIL: " + "; ".join(f"{v.row.entry.name} ({'; '.join(v.reasons)})" for v in failed)
    skipped = sum(verdict.status == "SKIP" for verdict in verdicts)
    measured = len(verdicts) - skipped
    new = sum(verdict.status == "NEW" for verdict in verdicts)
    up = sum(raisable(verdict) for verdict in verdicts)
    tail = " (make verify-ratchet locks them in)" if up else ""
    return f"VERIFY PASS: {measured} measured, {skipped} skipped, {new} new, {up} raisable{tail}"


def report(verdicts):
    """Print the skipped entries (loud, on stderr), the table and the verdict; the exit status for it."""
    for verdict in verdicts:
        if verdict.status == "SKIP":
            print(f"SKIP {verdict.row.entry.name}: {verdict.reasons[0]}", file=sys.stderr)
    print(format_table(verdicts))
    print(verdict_line(verdicts))
    return 1 if any(verdict.status == "FAIL" for verdict in verdicts) else 0


# Baseline and floors

def load_baseline():
    if not BASELINE.exists():
        return {}
    return {record["name"]: record for record in tomllib.loads(BASELINE.read_text()).get("repo", [])}


BASELINE_HEADER = """# Measured precision and recall per corpus entry: the ratchet. Written only by make verify-ratchet, which never
# lowers a number (scripts/corpus/verify.py). A record applies only while its sha and indexer_version match
# corpus.toml; a new pin starts again from its measured values. corpus.toml holds the inputs and the floors.
"""


def render_baseline(records):
    order = [entry.name for entry in corpus.select([])] + [SELF]
    names = [name for name in order if name in records] + sorted(set(records) - set(order))
    out = [BASELINE_HEADER]
    for name in names:
        record = records[name]
        out.append(f'\n[[repo]]\nname = "{name}"\nprecision = {record["precision"]:.4f}\n'
                   f'recall = {record["recall"]:.4f}\nsha = "{record["sha"]}"\n'
                   f'indexer_version = "{record["indexer_version"]}"\n')
    return "".join(out)


def record_of(row):
    return {"name": row.entry.name, "precision": row.precision, "recall": row.recall,
            "sha": row.pin, "indexer_version": row.entry.indexer_version}


def refusal_reasons(verdict, base):
    """Why the ratchet must not record VERDICT: a failure, an unmeasured metric, or a value below its comparable record."""
    row = verdict.row
    if verdict.status == "FAIL":
        return list(verdict.reasons)
    out = [f"{metric} unmeasured" for metric in METRICS if getattr(row, metric) is None]
    if comparable(base, row):
        out += [f"{metric} {getattr(row, metric):.4f} below baseline {base[metric]:.4f}" for metric in METRICS
                if getattr(row, metric) is not None and getattr(row, metric) < base[metric]]
    return out


def plan_ratchet(verdicts, baseline):
    """(refusals, notes, records): every measured entry recorded at its measured value; entries not measured keep theirs."""
    refusals, notes, records = [], [], dict(baseline)
    for verdict in verdicts:
        row, name = verdict.row, verdict.row.entry.name
        if row.skip and not row.problem:
            continue
        base = baseline.get(name)
        problems = refusal_reasons(verdict, base)
        if problems:
            refusals.append(f"{name}: {'; '.join(problems)}")
            continue
        if base is not None and not comparable(base, row):
            notes.append(f"{name}: pin or indexer changed; recorded numbers start again")
        records[name] = record_of(row)
    return refusals, notes, records


def plan_floors(verdicts, records):
    """The corpus.toml floors that rise: measured baseline minus the headroom, where that is above the floor now."""
    raised = {}
    for verdict in verdicts:
        entry, record = verdict.row.entry, records.get(verdict.row.entry.name)
        if verdict.status == "SKIP" or record is None:
            continue
        updates = {}
        for metric in METRICS:
            floor = getattr(entry, f"{metric}_floor")
            if floor is not None and corpus.suggested(record[metric]) > floor:
                updates[f"{metric}_floor"] = corpus.suggested(record[metric])
        if updates:
            raised[entry.name] = updates
    return raised


def set_floors(text, raised):
    """corpus.toml with the raised floors written in place; every other line is kept as it is."""
    out, current = [], None
    for line in text.splitlines(keepends=True):
        named = NAME_LINE.match(line)
        if named:
            current = named[1]
        floor = FLOOR_LINE.match(line)
        if floor and floor[1] in raised.get(current, {}):
            line = f"{floor[1]} = {raised[current][floor[1]]:.4f}\n"
        out.append(line)
    return "".join(out)


def cmd_ratchet(args):
    entries = selected(args)
    verdicts = run_scoreboard(entries, corpus.find_binary(args.bin), with_self=not (args.names or args.lang))
    refusals, notes, records = plan_ratchet(verdicts, load_baseline())
    for note in notes:
        print(f"ratchet: {note}")
    if refusals:
        for refusal in refusals:
            print(f"ratchet refused: {refusal}", file=sys.stderr)
        print("ratchet: nothing written", file=sys.stderr)
        return 1
    raised = plan_floors(verdicts, records)
    BASELINE.write_text(render_baseline(records))
    corpus.MANIFEST.write_text(set_floors(corpus.MANIFEST.read_text(), raised))
    print(f"ratchet: {BASELINE.name} records {len(records)} entries; floors raised: "
          f"{', '.join(sorted(raised)) or 'none'}")
    return 0


# Changed files

def changed_paths():
    """Tracked changes against HEAD plus untracked files, repo-relative."""
    tracked = subprocess.run(["git", "diff", "--name-only", "HEAD"], cwd=ROOT, capture_output=True,
                             text=True, check=True).stdout
    untracked = subprocess.run(["git", "ls-files", "--others", "--exclude-standard"], cwd=ROOT,
                               capture_output=True, text=True, check=True).stdout
    return sorted(set(filter(None, (tracked + untracked).splitlines())))


def scope_of(path, known):
    """The languages a change to PATH can move, ALL when it can move every entry, or an empty set when none."""
    parts = path.split("/")
    if path in WHOLE_TREE:
        return ALL
    if parts[0] == "src":
        if len(parts) > 3 and parts[1] == "languages" and parts[2] != "common":
            return {parts[2]}
        return ALL
    if len(parts) == 3 and parts[:2] == ["tests", "languages"] and parts[2].endswith(".rs"):
        stem = parts[2][:-3]
        return {lang for lang in known if stem == lang or stem.startswith(lang + "_")} or ALL
    return set()


def affected_languages(paths, known):
    langs = set()
    for path in paths:
        scope = scope_of(path, known)
        if scope is ALL:
            return ALL
        langs |= scope
    return langs


def changed_scope(args):
    """(entries, with_self) that the working tree's changes can move; NAME, LANG and git narrow it in that order."""
    entries = selected(args)
    if args.names:
        return entries, False
    langs = {args.lang} if args.lang else affected_languages(changed_paths(), known_languages())
    if langs is ALL:
        return entries, True
    return [entry for entry in entries if set(entry.languages) & langs], "rust" in langs


def cmd_changed(args):
    entries, with_self = changed_scope(args)
    if not entries and not with_self:
        print("verify changed: no corpus entry moves with these changes")
        return 0
    return scoreboard_and_diff(args, entries, with_self)


# Diff of verdicts between two binaries

def site_details(text):
    """The sampled disagreeing sites by (file:line, callee): the scip target and our binding for each."""
    found = {}
    for tag in SITE_TAG.findall(text):
        attrs = dict(corpus.ATTR.findall(tag))
        if "p" in attrs:
            found.setdefault((attrs["p"], attrs.get("callee", "")), attrs)
    return found


def diff_report(name, old_text, old_attrs, new_text, new_attrs):
    """The flips between two verdict dumps: a summary, each right->wrong site with ours and scip, and the counts."""
    result = corpus.flips(corpus.parse_states(old_text), corpus.parse_states(new_text))
    sites = site_details(new_text)
    lines = [f"{name}: precision {old_attrs.get('precision')} -> {new_attrs.get('precision')}, "
             f"recall {old_attrs.get('recall')} -> {new_attrs.get('recall')}"]
    for key in result["right->wrong"]:
        site = sites.get(key, {})
        lines.append(f"  right->wrong {key[0]} callee={key[1]} ours={site.get('ours', '-')} "
                     f"scip={site.get('scip', '-')}")
    counts = {label: len(result[label]) for label in corpus.FLIPS}
    return Diff(name, lines, counts)


def diff_one(entry, old_bin, new_bin):
    """The right->wrong flips for one entry, with ours and scip for each site the eval sampled."""
    if entry.status == "blocked":
        return Diff(entry.name, [], {}, skip=f"blocked: {entry.reason}")
    try:
        src, index, _ = locate(entry)
        old_text, old_attrs = corpus.scip_verdicts(old_bin, src, index)
        new_text, new_attrs = corpus.scip_verdicts(new_bin, src, index, SITE_SAMPLES)
    except corpus.Skip as exc:
        return Diff(entry.name, [], {}, skip=str(exc))
    return diff_report(entry.name, old_text, old_attrs, new_text, new_attrs)


def diff_entries(entries, old_bin, new_bin):
    """Print each entry's flips and the totals; 1 when any site flipped right->wrong."""
    if not entries:
        return 0
    with concurrent.futures.ProcessPoolExecutor(max_workers=min(os.cpu_count() or 1, len(entries))) as pool:
        results = list(pool.map(diff_one, entries, itertools.repeat(old_bin), itertools.repeat(new_bin)))
    totals = dict.fromkeys(corpus.FLIPS, 0)
    for result in results:
        if result.skip:
            print(f"SKIP {result.name}: {result.skip}", file=sys.stderr)
            continue
        print("\n".join(result.lines))
        for label, count in result.counts.items():
            totals[label] += count
    print("diff total: " + ", ".join(f"{label} {totals[label]}" for label in corpus.FLIPS))
    return 1 if totals["right->wrong"] else 0


def scoreboard_and_diff(args, entries, with_self):
    code = report(run_scoreboard(entries, corpus.find_binary(args.bin), with_self))
    if args.diff:
        code |= diff_entries([entry for entry in entries if entry.status != "blocked"],
                             corpus.find_binary(args.diff), corpus.find_binary(args.bin))
    return code


def cmd_run(args):
    return scoreboard_and_diff(args, selected(args), with_self=not (args.names or args.lang))


# Watch

def source_stamp():
    """A digest of every src/ file's path, size and mtime: it changes when a source file is edited, added or removed."""
    digest = hashlib.sha256()
    for dirpath, dirnames, filenames in os.walk(ROOT / "src"):
        dirnames.sort()
        for name in sorted(filenames):
            stat = os.stat(os.path.join(dirpath, name))
            digest.update(f"{dirpath}/{name}:{stat.st_mtime_ns}:{stat.st_size}\n".encode())
    return digest.hexdigest()


def rebuild():
    done = subprocess.run(["make", "-s", "verify-bin"], cwd=ROOT)
    if done.returncode:
        print("verify watch: build failed; still watching", file=sys.stderr)
    return done.returncode == 0


def print_deltas(seen, verdicts):
    """Only the entries whose numbers moved since the last round."""
    moved = []
    for verdict in verdicts:
        row = verdict.row
        now = (row.precision, row.recall, row.covered)
        before = seen.get(row.entry.name)
        if before != now:
            old_p = "-" if before is None else corpus.fmt(before[0])
            old_r = "-" if before is None else corpus.fmt(before[1])
            moved.append(f"{row.entry.name}: precision {old_p} -> {corpus.fmt(row.precision)}, "
                         f"recall {old_r} -> {corpus.fmt(row.recall)} ({verdict.status})")
    print("\n".join(moved) or "no deltas")


def watch_round(entries, binary, seen):
    """Rebuild, measure, then print the full table (first round) or only the deltas; returns the next round's numbers."""
    if not rebuild():
        return seen
    verdicts = run_scoreboard(entries, binary, with_self=False)
    if seen is None:
        report(verdicts)
    else:
        print_deltas(seen, verdicts)
    return {v.row.entry.name: (v.row.precision, v.row.recall, v.row.covered) for v in verdicts}


def cmd_watch(args):
    entries = selected(args)
    if not entries:
        sys.exit("verify watch: LANG= or NAME= selects no entries")
    binary = corpus.find_binary(args.bin)
    seen, stamp = None, None
    try:
        while True:
            current = source_stamp()
            if current != stamp:
                stamp = current
                seen = watch_round(entries, binary, seen)
            time.sleep(WATCH_POLL)
    except KeyboardInterrupt:
        return 0


# Status and warm

def tree_bytes(root):
    total = 0
    for dirpath, _, names in os.walk(root):
        for name in names:
            try:
                total += os.path.getsize(os.path.join(dirpath, name))
            except OSError:
                continue
    return total


def human(size):
    for unit in ("B", "KiB", "MiB"):
        if size < 1024:
            return f"{size:.1f} {unit}"
        size /= 1024
    return f"{size:.1f} GiB"


def cache_line():
    parts = [f"{sub} {human(tree_bytes(CACHE / sub))}" for sub in ("index", "src", "out", "logs", "resolution")
             if (CACHE / sub).exists()]
    return f"cache {CACHE}: {human(tree_bytes(CACHE))} ({', '.join(parts) or 'empty'})"


def image_built(image):
    return subprocess.run(["docker", "image", "inspect", image], capture_output=True).returncode == 0


def docker_lines(images):
    if shutil.which("docker") is None:
        return ["docker: not installed; indexes can only come from the cache"]
    if subprocess.run(["docker", "info", "--format", "{{.ServerVersion}}"], capture_output=True).returncode:
        return ["docker: installed, daemon not running"]
    return [f"docker: running; image {image} "
            f"{'built' if image_built(image) else 'MISSING (make corpus-image or corpus-image-lsp)'}"
            for image in sorted(images)]


def index_state(entry):
    """current, stale (cached for another pin or indexer), or missing; computed without fetching anything."""
    if entry.status == "blocked":
        return "blocked"
    key = index_key(entry.sha) if not entry.path else index_key(pin_of(entry)[1])
    index = corpus.index_path(entry, key)
    if index.exists():
        return f"current  {index.name}"
    others = sorted((CACHE / "index").glob(f"{entry.name}-*.scip"))
    if others:
        return f"stale    {others[-1].name} (pin or indexer changed; make verify-warm)"
    return "missing  make verify-warm"


def cmd_status(args):
    entries = selected(args)
    print("\n".join(docker_lines({entry.image for entry in entries})))
    print(cache_line())
    for entry in entries:
        print(f"{entry.name:<12} {index_state(entry)}")
    return 0


def cmd_warm(args):
    entries = selected(args)

    def action(entry):
        _, index = corpus.prepare(entry, force=False)
        print(f"{entry.name}: {index.name}")

    skipped = corpus.run_entries(entries, action)
    print(f"warm: {len(entries) - len(skipped)} ready, {len(skipped)} skipped")
    return 0


HANDLERS = {
    "run": cmd_run,
    "ratchet": cmd_ratchet,
    "changed": cmd_changed,
    "watch": cmd_watch,
    "status": cmd_status,
    "warm": cmd_warm,
}


def build_parser():
    parser = argparse.ArgumentParser(prog="corpus.py verify", description=__doc__.split("\n")[0])
    parser.add_argument("mode", nargs="?", default="run", choices=MODES)
    parser.add_argument("names", nargs="*", help="entries to verify (NAME=...)")
    parser.add_argument("--bin", help="ripwire-rs binary to score (default target/release/ripwire-rs)")
    parser.add_argument("--lang", help="only entries for this language (LANG=...)")
    parser.add_argument("--diff", nargs="?", const=str(BASELINE_BIN), metavar="OLD_BIN",
                        help="also list the sites that flipped right->wrong against OLD_BIN")
    return parser


def main(argv=None):
    parser = build_parser()
    args = parser.parse_args(argv)
    if args.diff and args.mode not in ("run", "changed"):
        parser.error("--diff applies to run and changed")
    return HANDLERS[args.mode](args)


if __name__ == "__main__":
    sys.exit(main())
