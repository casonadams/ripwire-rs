#!/usr/bin/env python3
"""Score ripwire-rs `.sqlx` table lineage against what the Dataform compiler says each model reads.

`--eval=scip` scores only references the parser already extracted, so it cannot see a read the parser never
emitted. This script compares edge sets instead: truth is every (consumer, table) pair the compiled
output implies, found is every pair `--uses TABLE` reports. Truth for A reading T is any of:
  - T is in A's compiled `dependencyTargets` (ref(...) and config dependencies), or
  - T's qualified name (`db.schema.name` or `schema.name`) appears, outside comments, in a query A compiles to
    (this covers resolve(...), `${database}.schema.table` and literal FROM/JOIN paths).
Only actions defined in `.sqlx` files count: ripwire does not parse `.js` actions.

Usage: dataform_eval.py --root DIR --bin ripwire-rs [--compiled FILE | --dataform "npx --yes @dataform/cli@3"]
"""

import argparse
import json
import pathlib
import re
import shlex
import subprocess
import sys

ACTION_KINDS = ("tables", "views", "assertions", "operations", "declarations")
QUERY_FIELDS = ("query", "incrementalQuery")
QUERY_LISTS = ("preOps", "postOps", "queries")
USE_ROW = re.compile(r'<u role="read" p="([^"]+):\d+" in_id="([^"]+)"')


def string_end(sql, start):
    quote, end = sql[start], start + 1
    while end < len(sql) and sql[end] != quote:
        end += 2 if sql[end] == "\\" else 1
    return end + 1


def comment_end(sql, start):
    """Index just past the comment opening at `start`, or None when none opens there."""
    if sql.startswith("--", start):
        end = sql.find("\n", start)
        return len(sql) if end < 0 else end
    if sql.startswith("/*", start):
        end = sql.find("*/", start + 2)
        return len(sql) if end < 0 else end + 2
    return None


def strip_sql_comments(sql):
    """`sql` without `--` and `/* */` comments, leaving string literals intact."""
    out, i = [], 0
    while i < len(sql):
        end = comment_end(sql, i)
        if end is not None:
            i = end
            continue
        stop = string_end(sql, i) if sql[i] in "'\"`" else i + 1
        out.append(sql[i:stop])
        i = stop
    return "".join(out)


def queries_of(action):
    texts = [action[f] for f in QUERY_FIELDS if isinstance(action.get(f), str)]
    for field in QUERY_LISTS:
        texts.extend(t for t in action.get(field) or [] if isinstance(t, str))
    return [strip_sql_comments(t).replace("`", "") for t in texts]


def qualified_names(target):
    parts = [target.get("database"), target.get("schema"), target["name"]]
    return [".".join(p for p in parts[i:] if p) for i in (0, 1)]


def mentions(query, target):
    return any(
        re.search(rf"(?<![\w.-]){re.escape(name)}(?![\w])", query)
        for name in qualified_names(target)
    )


def sqlx_actions(compiled):
    """(file, target) for each action defined in a `.sqlx` file; first action wins when a file holds several."""
    seen = {}
    for kind in ACTION_KINDS:
        for action in compiled.get(kind) or []:
            file = action.get("fileName") or ""
            if file.endswith(".sqlx") and file not in seen:
                seen[file] = (action, action["target"])
    return seen


def target_key(target):
    return tuple(sorted(target.items()))


def reads_of(action, targets):
    """Keys of the compiled targets `action` reads, by dependency or by a qualified name in a query."""
    queries = queries_of(action)
    reads = {target_key(d) for d in action.get("dependencyTargets") or []}
    reads |= {key for key, t in targets.items() if any(mentions(q, t) for q in queries)}
    return reads & targets.keys()


def truth_edges(compiled):
    """{(consumer_name, table_name)} the compiled project implies, for `.sqlx` actions only."""
    actions = sqlx_actions(compiled)
    targets = {target_key(t): t for _, t in actions.values()}
    return {
        (consumer["name"], targets[key]["name"])
        for action, consumer in actions.values()
        for key in reads_of(action, targets)
        if targets[key]["name"] != consumer["name"]
    }


def found_edges(binary, root, tables):
    edges, ambiguous = set(), []
    for table in sorted(tables):
        run = subprocess.run([binary, root, "--uses", table], capture_output=True, text=True)
        out = run.stdout
        if run.returncode != 0 or 'defs="1"' not in out:
            ambiguous.append(table)
            continue
        edges |= {(consumer, table) for _, consumer in USE_ROW.findall(out) if consumer != table}
    return edges, ambiguous


def score(truth, found):
    hit = truth & found
    return {
        "truth": len(truth),
        "found": len(found),
        "precision": len(hit) / len(found) if found else None,
        "recall": len(hit) / len(truth) if truth else None,
        "missed": sorted(truth - found),
        "false_edges": sorted(found - truth),
    }


def compile_project(root, command):
    run = subprocess.run([*shlex.split(command), "compile", "--json"], cwd=root, capture_output=True, text=True)
    try:
        return json.loads(run.stdout)
    except json.JSONDecodeError:
        raise SystemExit(f"dataform_eval: compile produced no JSON (exit {run.returncode}): {run.stderr[:400]}")


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    parser.add_argument("--root", required=True)
    parser.add_argument("--bin", default="ripwire-rs")
    parser.add_argument("--compiled", help="compile --json output to use instead of running the compiler")
    parser.add_argument("--dataform", default="npx --yes @dataform/cli@3")
    args = parser.parse_args(argv)
    if args.compiled:
        compiled = json.loads(pathlib.Path(args.compiled).read_text())
    else:
        compiled = compile_project(args.root, args.dataform)
    names = {target["name"] for _, target in sqlx_actions(compiled).values()}
    truth = truth_edges(compiled)
    found, ambiguous = found_edges(args.bin, args.root, names)
    found = {(c, t) for c, t in found if c in names}
    result = score(truth, found)
    result["tables_without_one_definition"] = ambiguous
    print(json.dumps(result, indent=1))


if __name__ == "__main__":
    sys.exit(main())
