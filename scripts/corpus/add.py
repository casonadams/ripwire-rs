#!/usr/bin/env python3
"""Add or retire a corpus repository (reached as `corpus.py add` and `corpus.py remove`).

add checks the repository on GitHub (a permissive license, a pinned full commit SHA), fetches that commit
shallowly into the cache, counts the language's source files, and appends an unverified [[repo]] to corpus.toml
from the language's template in languages.toml. --measure then indexes and scores the entry; on success it writes
floors (measured minus MARGIN, rounded down) and marks it measured. Failure leaves it unverified.

remove deletes the manifest entry, its cached checkout and index, and its baseline.toml row.

  corpus.py add URL --language LANG [--name N] [--sha SHA] [--path SUBDIR] [--measure] [--dry-run] [--bin PATH]
  corpus.py add --list-languages
  corpus.py remove NAME [--yes]
"""

import argparse
import json
import os
import pathlib
import re
import shutil
import sys
import tomllib
import urllib.error
import urllib.request

import corpus

LANGUAGES = pathlib.Path(__file__).with_name("languages.toml")
BASELINE = pathlib.Path(__file__).with_name("baseline.toml")
GATES = corpus.ROOT / "docs" / "GATES.md"
API = "https://api.github.com"
ALLOWED_LICENSES = ("MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Unlicense", "0BSD")
SOURCE_GUIDE = (50, 300)
MIN_SOURCE_FILES = 10
GITHUB_URL = re.compile(r"https://github\.com/([A-Za-z0-9_.-]+)/([A-Za-z0-9_.-]+?)(?:\.git)?/?")
SHA = re.compile(r"[0-9a-f]{40}")
NAME = re.compile(r"[a-z0-9][a-z0-9._-]{0,62}")
UNVERIFIED = 'status = "unverified"'
CONTROL = re.compile(r"[\x00-\x08\x0b-\x1f\x7f]")
ENTRY_ORDER = (
    "name", "languages", "status", "license", "url", "sha", "subdir", "indexer", "indexer_version",
    "setup", "index", "image", "precision_floor", "recall_floor",
)
MATRIX_HEADER = "| Language | Oracle | Status | Precision | Recall |"


class AddError(Exception):
    """A refusal or failure with a message for the user; the command exits 1."""


def warn(message):
    print(f"warning: {message}", file=sys.stderr)


def parse_url(url):
    match = GITHUB_URL.fullmatch(url.strip())
    if match is None:
        raise AddError(f"expected https://github.com/OWNER/REPO, got {url!r}")
    return match[1], match[2]


def github_get(path, token):
    request = urllib.request.Request(API + path, headers={
        "Accept": "application/vnd.github+json", "User-Agent": "ripwire-rs-corpus",
    })
    if token:
        request.add_header("Authorization", f"Bearer {token}")
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            return json.load(response)
    except urllib.error.HTTPError as error:
        raise AddError(f"GitHub API {path}: HTTP {error.code}") from error
    except urllib.error.URLError as error:
        raise AddError(f"GitHub API {path}: {error.reason}") from error


def github_api():
    token = os.environ.get("GITHUB_TOKEN")
    return lambda path: github_get(path, token)


def git_runner(args, cwd):
    return corpus.must(args, " ".join(args[:2]), cwd=cwd)


def repo_facts(owner, repo, api):
    data = api(f"/repos/{owner}/{repo}")
    return {
        "license": (data.get("license") or {}).get("spdx_id") or "",
        "default_branch": data.get("default_branch") or "",
        "archived": bool(data.get("archived")),
        "fork": bool(data.get("fork")),
    }


def warnings_for(facts):
    out = []
    if facts["archived"]:
        out.append("the repository is archived: its pinned commit will not change, and nobody will fix it upstream")
    if facts["fork"]:
        out.append("the repository is a fork: prefer the upstream project unless this fork is the one you mean")
    return out


def check_license(spdx):
    if spdx in ALLOWED_LICENSES:
        return spdx
    if spdx in ("", "NOASSERTION"):
        raise AddError("GitHub reports no single SPDX license for this repository; add only one with a clear "
                       "permissive license")
    raise AddError(f"license {spdx} is not on the permissive allowlist: {', '.join(ALLOWED_LICENSES)}")


def validate_sha(sha):
    if not SHA.fullmatch(sha or ""):
        raise AddError(f"a commit SHA must be 40 lowercase hex characters, got {sha!r}")
    return sha


def resolve_sha(owner, repo, facts, given, api):
    if given:
        validate_sha(given)
    ref = given or facts["default_branch"]
    if not ref:
        raise AddError("GitHub reports no default branch: pass --sha")
    sha = api(f"/repos/{owner}/{repo}/commits/{ref}").get("sha", "")
    if given and sha != given:
        raise AddError(f"commit {given} does not exist in {owner}/{repo}")
    return validate_sha(sha)


def fetch_source(name, url, sha, git):
    """Shallow-fetch SHA into the cache checkout of NAME and check it out."""
    dest = corpus.CACHE / "src" / name
    if not (dest / ".git").is_dir():
        dest.mkdir(parents=True, exist_ok=True)
        git(["git", "init", "-q"], dest)
        git(["git", "remote", "add", "origin", url], dest)
    else:
        git(["git", "remote", "set-url", "origin", url], dest)
    git(["git", "fetch", "-q", "--depth", "1", "origin", sha], dest)
    git(["git", "checkout", "-q", "-f", "--detach", "FETCH_HEAD"], dest)
    git(["git", "clean", "-q", "-ffdx"], dest)
    return dest


def tracked_files(src, git):
    listing = git(["git", "ls-files", "-z"], src)
    return [path for path in listing.split("\0") if path]


def source_count(paths, extensions, subdir=""):
    """Files under SUBDIR (the whole repository when empty) whose extension the language recognizes."""
    folder = subdir.strip("/")
    prefix = f"{folder}/" if folder else ""
    wanted = {ext.lower() for ext in extensions}
    return sum(1 for path in paths
               if path.startswith(prefix) and pathlib.PurePosixPath(path).suffix[1:].lower() in wanted)


def size_verdict(count):
    low, high = SOURCE_GUIDE
    if count < MIN_SOURCE_FILES:
        return "error", f"{count} source files: too few to score (at least {MIN_SOURCE_FILES})"
    if count < low or count > high:
        return "warn", f"{count} source files: outside the {low}-{high} guide"
    return "ok", f"{count} source files"


def load_languages():
    return tomllib.loads(LANGUAGES.read_text())


def template_for(language, table):
    if language not in table:
        raise AddError(f"unknown language {language!r}: run corpus.py add --list-languages")
    template = table[language]
    if template["status"] == "none":
        raise AddError(f"{language} has no indexer to score: {template['reason']}")
    if template["status"] == "blocked":
        raise AddError(f"{language} is blocked: {template['reason']}")
    return template


def oracle_label(template):
    if template["oracle"] == "lsp":
        return f"LSP bridge ({template['server']})"
    return f"SCIP indexer ({template['indexer']})"


def default_name(repo):
    return re.sub(r"[^a-z0-9._-]+", "-", repo.lower()).strip("-.")


def manifest_entries(text):
    return tomllib.loads(text).get("repo", [])


def normal_url(url):
    return url.lower().rstrip("/").removesuffix(".git")


def check_unique(entries, name, url):
    for table in entries:
        if table["name"] == name:
            raise AddError(f"the manifest already has an entry named {name}")
        if url and normal_url(table.get("url", "")) == normal_url(url):
            raise AddError(f"the manifest already has this repository as {table['name']}")


def check_name(name):
    if not NAME.fullmatch(name):
        raise AddError(f"name {name!r} must match {NAME.pattern}")


def entry_fields(name, url, language, template, license_id, sha, subdir):
    fields = {
        "name": name, "languages": [language], "status": "unverified", "license": license_id,
        "url": url, "sha": sha, "subdir": subdir,
        "indexer": template["indexer"], "indexer_version": template["indexer_version"],
        "setup": template.get("setup", "").replace("{project}", name),
        "index": template["index"].replace("{project}", name),
    }
    if template.get("image", corpus.IMAGE) != corpus.IMAGE:
        fields["image"] = template["image"]
    return fields


def toml_value(value):
    if isinstance(value, float):
        return f"{value:.4f}"
    if isinstance(value, list):
        return "[" + ", ".join(json.dumps(item) for item in value) + "]"
    literal = ("\n" in value and "'''" not in value and not value.startswith("\n")
               and not value.endswith("'") and not CONTROL.search(value))
    return f"'''{value}'''" if literal else json.dumps(value)


def render_entry(fields):
    lines = ["[[repo]]"]
    for key in ENTRY_ORDER:
        value = fields.get(key)
        if value not in (None, "", []):
            lines.append(f"{key} = {toml_value(value)}")
    return "\n".join(lines) + "\n"


def separator(text):
    if not text or text.endswith("\n\n"):
        return ""
    return "\n" if text.endswith("\n") else "\n\n"


def append_entry(text, block):
    """TEXT with BLOCK appended. Every existing byte is kept; the result must parse with one more entry."""
    before = len(manifest_entries(text))
    new = text + separator(text) + block
    if len(manifest_entries(new)) != before + 1:
        raise AddError("the appended entry did not parse as one new [[repo]] table")
    return new


def block_span(text, name):
    """Byte span of the [[repo]] table named NAME, through the blank lines that follow it; None when absent."""
    match = re.search(r"^\[\[repo\]\]\nname = " + re.escape(json.dumps(name)) + r"\n", text, re.M)
    if match is None:
        return None
    following = text.find("\n[[repo]]", match.end())
    return match.start(), (len(text) if following < 0 else following + 1)


def floor_for(value):
    return max(0.0, corpus.suggested(value))


def mark_measured(text, name, precision, recall):
    span = block_span(text, name)
    if span is None:
        raise AddError(f"no manifest entry named {name}")
    block = text[span[0]:span[1]]
    if UNVERIFIED not in block:
        raise AddError(f"{name} is not unverified; refusing to change its status")
    body = block.replace(UNVERIFIED, 'status = "measured"', 1).rstrip("\n")
    trailing = block[len(block.rstrip("\n")):]
    body += f"\nprecision_floor = {floor_for(precision):.4f}\nrecall_floor = {floor_for(recall):.4f}"
    return text[:span[0]] + body + trailing + text[span[1]:]


def remove_block(text, name):
    span = block_span(text, name)
    if span is None:
        return text
    return text[:span[0]] + text[span[1]:]


def recorded_numbers(entries):
    """Precision and recall per entry from baseline.toml, only where its sha and indexer_version still match."""
    if not BASELINE.exists():
        return {}
    records = {row["name"]: row for row in tomllib.loads(BASELINE.read_text()).get("repo", [])}
    out = {}
    for table in entries:
        row = records.get(table["name"])
        if row and row["sha"] == table.get("sha") and row["indexer_version"] == table.get("indexer_version"):
            out[table["name"]] = (row["precision"], row["recall"])
    return out


def span_text(values):
    low, high = min(values), max(values)
    return f"{low:.4f}" if low == high else f"{low:.4f}-{high:.4f}"


def matrix_bounds(lines):
    if MATRIX_HEADER not in lines:
        return 0, 0
    start = lines.index(MATRIX_HEADER)
    end = start + 2
    while end < len(lines) and lines[end].startswith("|"):
        end += 1
    return start, end


def matrix_index(lines, language):
    start, end = matrix_bounds(lines)
    for index in range(start, end):
        if lines[index].startswith(f"| {language} |"):
            return index
    return None


def unverified_row(language, template):
    return f"| {language} | {oracle_label(template)} | unverified | - | - |"


def measured_row(language, template, entries, numbers, gates_lines):
    """The matrix row for LANGUAGE over its measured entries, or None when none of them has numbers."""
    measured = [t for t in entries if language in t.get("languages", []) and t.get("status") == "measured"
                and t["name"] in numbers]
    if not measured:
        return None
    precisions = [numbers[t["name"]][0] for t in measured]
    recalls = [numbers[t["name"]][1] for t in measured]
    index = matrix_index(gates_lines, language)
    cells = [cell.strip() for cell in gates_lines[index].split("|")] if index is not None else []
    oracle = cells[2] if len(cells) > 2 else oracle_label(template)
    status = cells[3] if len(cells) > 3 and cells[3].startswith("measured") else "measured"
    return f"| {language} | {oracle} | {status} | {span_text(precisions)} | {span_text(recalls)} |"


def update_matrix(text, language, row):
    """TEXT with the matrix row of LANGUAGE replaced by ROW; the same text when the language has no row."""
    lines = text.split("\n")
    index = matrix_index(lines, language)
    if index is None:
        return text, False
    lines[index] = row
    return "\n".join(lines), True


def list_languages():
    table = load_languages()
    groups = (("templates", "template"), ("blocked", "blocked"), ("no indexer", "none"))
    for title, status in groups:
        print(f"{title}:")
        for language, template in sorted(table.items()):
            if template["status"] != status:
                continue
            detail = oracle_label(template) if status == "template" else template["reason"]
            print(f"  {language:<12} {detail}")
    return 0


def confirm(prompt):
    return sys.stdin.isatty() and input(prompt).strip().lower() == "y"


def report_failure(name, exc):
    print(f"add: {name} stays unverified: {exc}", file=sys.stderr)
    log = corpus.CACHE / "logs" / f"{name}.log"
    if not log.exists():
        return
    lines = log.read_text(errors="replace").splitlines()
    errors = [line for line in lines if re.search(r"error|exception|fatal|failed", line, re.I)][:5]
    print(f"log: {log}", file=sys.stderr)
    for line in errors or lines[-5:]:
        print(f"  {line}", file=sys.stderr)


def update_gates(language, template, entries, numbers):
    """Rewrite the matrix row for LANGUAGE in docs/GATES.md when it exists; return the row for the user."""
    text = GATES.read_text()
    row = measured_row(language, template, entries, numbers, text.split("\n"))
    updated, found = update_matrix(text, language, row) if row else (text, False)
    if found:
        GATES.write_text(updated)
    return row


def measure_added(name, binary, language, template):
    entries = manifest_entries(corpus.MANIFEST.read_text())
    try:
        score = corpus.score_entry(corpus.select([name])[0], corpus.find_binary(binary))
    except corpus.Skip as exc:
        report_failure(name, exc)
        return 1
    if score.precision is None or score.recall is None:
        report_failure(name, "the scorer reported no precision or recall")
        return 1
    corpus.MANIFEST.write_text(mark_measured(corpus.MANIFEST.read_text(), name, score.precision, score.recall))
    print(f"{name}: calls {score.calls}, covered {score.covered}, precision {score.precision:.4f}, "
          f"recall {score.recall:.4f}")
    numbers = recorded_numbers(entries)
    numbers[name] = (score.precision, score.recall)
    row = update_gates(language, template, manifest_entries(corpus.MANIFEST.read_text()), numbers)
    print(f"matrix row: {row or unverified_row(language, template)}")
    print("next: make verify-ratchet records the baseline; then make verify")
    return 0


def add_entry(args, api, git):
    table = load_languages()
    template = template_for(args.language, table)
    owner, repo = parse_url(args.url)
    entries = manifest_entries(corpus.MANIFEST.read_text())
    name = args.name or default_name(repo)
    check_name(name)
    url = f"https://github.com/{owner}/{repo}"
    check_unique(entries, name, url)
    facts = repo_facts(owner, repo, api)
    for note in warnings_for(facts):
        warn(note)
    license_id = check_license(facts["license"])
    sha = resolve_sha(owner, repo, facts, args.sha, api)
    src = fetch_source(name, url, sha, git)
    count = source_count(tracked_files(src, git), template["extensions"], args.subdir)
    level, message = size_verdict(count)
    if level == "error":
        raise AddError(message)
    if level == "warn":
        warn(f"{message}; pass --path SUBDIR to index one directory of the repository")
    block = render_entry(entry_fields(name, url, args.language, template, license_id, sha, args.subdir))
    print(f"{name}: {license_id}, {sha[:12]}, {message}")
    if args.dry_run:
        print(block, end="")
        print(f"matrix row: {unverified_row(args.language, template)}")
        return 0
    corpus.MANIFEST.write_text(append_entry(corpus.MANIFEST.read_text(), block))
    print(f"added {name} as unverified in corpus.toml")
    if args.measure:
        return measure_added(name, args.bin, args.language, template)
    print(f"next: make corpus-index NAME={name}; make corpus-measure NAME={name}; make verify-ratchet")
    return 0


def cache_paths(name):
    """The cached checkout, output, log and index files of NAME that exist now."""
    paths = [corpus.CACHE / "src" / name, corpus.CACHE / "out" / name, corpus.CACHE / "logs" / f"{name}.log"]
    index_dir = corpus.CACHE / "index"
    if index_dir.exists():
        index_file = re.compile(re.escape(name) + r"-[0-9a-f]{12}-.*\.scip")
        paths += [path for path in index_dir.iterdir() if index_file.fullmatch(path.name)]
    return [path for path in paths if path.exists()]


def delete_path(path):
    if path.is_dir():
        shutil.rmtree(path, ignore_errors=True)
    else:
        path.unlink()


def remove_repo(name, yes):
    text = corpus.MANIFEST.read_text()
    table = next((t for t in manifest_entries(text) if t["name"] == name), None)
    if table is None:
        raise AddError(f"no manifest entry named {name}")
    if not table.get("url"):
        raise AddError(f"{name} is a local tree (path), not a pinned repository; refusing to remove it")
    baseline = BASELINE.read_text() if BASELINE.exists() else ""
    has_baseline = block_span(baseline, name) is not None
    cached = cache_paths(name)
    print(f"remove {name}: corpus.toml entry" + ("; baseline.toml row" if has_baseline else ""))
    for path in cached:
        print(f"  {path}")
    if not yes and not confirm(f"remove {name}? [y/N] "):
        print("aborted: nothing removed (pass --yes to skip the prompt)")
        return 1
    corpus.MANIFEST.write_text(remove_block(text, name))
    if has_baseline:
        BASELINE.write_text(remove_block(baseline, name))
    for path in cached:
        delete_path(path)
    print(f"removed {name}; if it was the only measured entry for its language, edit that docs/GATES.md row")
    return 0


ADD_OPTIONS = (
    ("--language", {"help": "the language the entry is scored for (see --list-languages)"}),
    ("--name", {"help": "entry name (default: the repository name)"}),
    ("--sha", {"help": "full 40-hex commit (default: the default branch HEAD)"}),
    ("--path", {"dest": "subdir", "default": "", "help": "index only this directory of the repository"}),
    ("--measure", {"action": "store_true", "help": "index and score the entry; floors on success"}),
    ("--dry-run", {"action": "store_true", "help": "check and print the entry; change nothing"}),
    ("--list-languages", {"action": "store_true", "help": "templates, blocked and no-indexer languages"}),
    ("--bin", {"help": "ripwire-rs binary for --measure (default target/release/ripwire-rs)"}),
)
REMOVE_OPTIONS = (("--yes", {"action": "store_true", "help": "do not prompt"}),)


def add_subcommand(sub, name, help_text, positional, options):
    child = sub.add_parser(name, help=help_text)
    for argument in positional:
        child.add_argument(argument[0], **argument[1])
    for flag, extra in options:
        child.add_argument(flag, **extra)
    return child


def build_parser():
    parser = argparse.ArgumentParser(prog="corpus.py", description="Add or retire a corpus repository.")
    sub = parser.add_subparsers(dest="cmd", required=True)
    add_subcommand(sub, "add", "add a GitHub repository as an unverified corpus entry",
                   (("url", {"nargs": "?"}),), ADD_OPTIONS)
    add_subcommand(sub, "remove", "retire a corpus entry: manifest, cache, index and baseline row",
                   (("name", {}),), REMOVE_OPTIONS)
    return parser


def add_command(args, api, git):
    if args.list_languages:
        return list_languages()
    if not args.url or not args.language:
        raise AddError("usage: corpus.py add URL --language LANG (see --list-languages)")
    if args.dry_run and args.measure:
        raise AddError("--dry-run and --measure cannot be combined: a dry run changes nothing")
    return add_entry(args, api, git)


def main(argv, api=None, git=None):
    args = build_parser().parse_args(argv)
    try:
        if args.cmd == "remove":
            return remove_repo(args.name, args.yes)
        return add_command(args, api or github_api(), git or git_runner)
    except (AddError, corpus.Skip) as exc:
        print(f"corpus.py: {exc}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
