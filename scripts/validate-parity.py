#!/usr/bin/env python3
import collections
import json
import os
import re
import shlex
import shutil
import subprocess
import sys
import tempfile
import tomllib
import xml.etree.ElementTree as ET
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent
LEDGER = ROOT / "scripts" / "parity-divergences.toml"
KINDS = ("smaller", "richer", "fixes-upstream-defect", "defect")
LEDGER_FIELDS = ("id", "command", "case", "kind", "why", "evidence")
EVIDENCE_RE = re.compile(r"(tests/[A-Za-z0-9_/]+\.rs)::([A-Za-z_][A-Za-z0-9_]*)")
OMIT_TOKEN_RE = re.compile(r"(?:[A-Za-z0-9_-]+:)?[A-Za-z0-9_-]+")
PAGING_ATTRS = ("total", "has_more", "next_offset", "capped")
ROW_KEYS = ("p", "n", "path", "name")
FULL_ROWS_FLAG = "--limit=100000"
FULL_TIMEOUT = 60
UPSTREAM_TIMEOUT = int(os.environ.get("PARITY_UPSTREAM_TIMEOUT", "4"))
WARNINGS = []

OPTIONAL_ATTRS = {
    "shown",
    "capped",
    "total",
    "has_more",
    "next_offset",
    "offset",
    "limit",
    "next",
    "graph_unindexed",
    "graph_ambiguous",
    "graph_unresolved",
    "graph_partial",
    "skipped_mode_only",
    "tsconfig_unread",
    "imports_unresolved",
    "ts_decl_only",
    "ts_extends_unread",
    "declined_calls",
    "builtins_excluded",
    "renames",
    "rename_window_commits",
    "acked_by_rename",
    "acked_by_content",
    "register-macro-excluded",
    "exempt_groups",
    "demoted_groups",
    "acks_bad_lines",
    "head_basis",
    "layer",
    "lazy_edges",
    "value-ref-excluded",
    "runner-root-excluded",
    "decorated-excluded",
    "declined-call-excluded",
    "shown_bridges",
    "bridges_capped",
    "bridges",
    "shown_importers",
    "importers_capped",
    "importers",
    "unavailable",
    "unavailable_why",
    "unavail",
    "reason",
    "at",
    "root",
    "pr_converged",
    "sites_at",
    "sites_total",
    "amb",
    "prov",
    "sc",
    "t_bytes",
    "anchoring",
    "dep_capable",
    "surprising",
    "driver",
    "hops",
    "complete",
    "suppressed_comment",
    "suppressed_string",
    "tier_parsed",
    "tier_unclassified",
    "counts_floor",
    "defs_capped",
    "fanout_cut",
    "depth_truncated",
    "date",
    "files",
    "superseded",
    "tip",
    "corpus_pruned_dirs",
    "unindexed_hits",
    "unindexed_files_scanned",
    "unindexed_files_skipped",
    "unindexed_candidates_capped",
    "hits_capped",
    "tested",
    "shown_unflagged",
    "unflagged_capped",
    "bodyless_defs",
    "defs",
    "dist",
    "cells_capped",
    "cells_shown",
    "findings_capped",
    "inert_rules",
    "doc_mentions",
    "doc_mentions_capped",
    "doc_mentions_total",
    "importers_next",
    "files_next",
    "dc_next",
    "dc_shown",
    "uses_next",
    "rows_next",
    "coverage",
    "tier_budget",
    "tier_files",
    "corpus_oversize",
    "notes_degraded",
    "parse_degraded",
    "join",
    "origin",
    "exempt",
    "idiom",
    "demoted",
    "filter",
    "unranked_extent_suspect",
    "lines_skipped",
    "as_stem_ambiguous",
    "as_stem_nonptr",
    "as_stem_unowned",
    "tier",
    "data_sections_cut",
    "run_first",
    "doc_links",
    "head_date",
    "listing",
    "ref_labels",
    "worktree",
    "by_depth",
    "callers_floor",
    "sev",
    "members",
    "tokens",
    "churn",
    "facet",
    "which",
    "which_version",
    "hint",
}


def report_error(message):
    escaped = message.replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")
    print(f"::error::{escaped}", file=sys.stderr)


def find_binaries():
    rw_bin = os.path.abspath(os.environ.get("RIPWIRE_BIN", "target/release/ripwire-rs"))
    if not os.path.exists(rw_bin):
        rw_bin = os.path.abspath("target/release/rw")
    if not os.path.exists(rw_bin):
        rw_bin = shutil.which("ripwire-rs") or shutil.which("rw")
    rip_bin = os.path.expanduser("~/.local/bin/ripwire")
    if not os.path.exists(rip_bin):
        rip_bin = shutil.which("ripwire")
    if not rw_bin or not rip_bin:
        report_error("ripwire-rs or ripwire binary not found.")
        sys.exit(1)
    rw_dir = os.path.dirname(os.path.abspath(rw_bin))
    if rw_dir not in os.environ.get("PATH", "").split(os.pathsep):
        os.environ["PATH"] = f"{rw_dir}{os.pathsep}{os.environ.get('PATH', '')}"
    return rw_bin, rip_bin


# Base-branch candidates for --merge-scout, in preference order. A PR checkout is a detached merge
# commit with only origin/* refs, so the remote-tracking spellings must be tried too.
MERGE_BASE_CANDIDATES = ("main", "master", "origin/main", "origin/master")
# The ledger and the tree-entry key always spell the base as `main`; the checker substitutes the
# ref it detected when it runs the command, so entries stay valid whatever spelling resolved.
MERGE_SCOUT_CMD = "--merge-scout=main"


def resolves_to_commit(repo_dir, rev):
    res = subprocess.run(
        ["git", "-C", repo_dir, "rev-parse", "--verify", "--quiet", f"{rev}^{{commit}}"],
        capture_output=True,
    )
    return res.returncode == 0


def detect_merge_ref(repo_dir):
    """First base candidate that resolves, or "HEAD" when none does (nothing to scout against)."""
    for candidate in MERGE_BASE_CANDIDATES:
        if resolves_to_commit(repo_dir, candidate):
            return candidate
    return "HEAD"


PINNED_TARGETS = ("row_bytes", "fit_rows")


def function_symbols(stdout):
    for pattern in (
        r'<s t="fn" n="([a-zA-Z_][a-zA-Z0-9_]*)"',
        r'<s t="method" n="([a-zA-Z_][a-zA-Z0-9_]*)"',
        r'<s [^>]*n="([a-zA-Z_][a-zA-Z0-9_]*)"',
    ):
        found = re.findall(pattern, stdout)
        if found:
            return found
    return []


def symbol_pool(fn_syms):
    counts = collections.Counter(fn_syms)
    unique_fns = [s for s in fn_syms if counts[s] == 1 and s not in ("new", "main", "name", "init")]
    pool = unique_fns if unique_fns else fn_syms
    # The ledger's per-symbol entries are measured on these two; a near-tie in PageRank must not swap them.
    pinned = [s for s in PINNED_TARGETS if s in pool]
    return pinned + [s for s in pool if s not in pinned]


def discover_repo_targets(rw_bin, repo_dir):
    proc = subprocess.run([rw_bin, repo_dir, "--top-k=50"], capture_output=True, text=True)
    files = re.findall(r'<f p="([^"]+)"', proc.stdout)
    pool = symbol_pool(function_symbols(proc.stdout))
    target_file = files[0] if files else "src/main.rs"
    target_sym = pool[0] if pool else "main"
    second_sym = pool[1] if len(pool) > 1 else target_sym
    return target_file, target_sym, second_sym


# Synthetic changed sets: existing files passed as CLI args, so the tree is never written.
# A Rust test_ basename under src/ is a module, not a partner (06ecbed), so the partner probe is a Python test_x pair.
CHANGED_SETS = (
    "src/gate/affected.rs,src/serialize/arch_xml/mod.rs",
    "scripts/validate-parity.py",
    "src/commands/situ.rs,src/commands/edit_check.rs,src/commands/safe_delete.rs",
)
# --edit-check and --safe-delete take one symbol each, so each symbol is its own run.
GATE_SYMBOLS = ("resolve_changed_or_spec_files", "build_handoff_verified_rows", "execute_test_gate_query_paged")
# Ranked lists: rows are compared by position, so a shape mismatch at any rank fails.
# Row identity order is not gated here; ranking differs from upstream and is reported, not ledgered.
RANKED_FLAGS = ("", "--top-k", "--token-budget", "--rank-by", "--biggest-first", "--hotspots", "--affected")


def build_command_list(target_file, target_sym, second_sym):
    return [
        ("", "Default ranked map"),
        ("--top-k=50", "Top 50 symbols"),
        ("--top-k=100", "Top 100 symbols"),
        ("--top-k=20", "Top 20 symbols"),
        ("--token-budget=2000", "2000 token budget"),
        ("--token-budget=4000", "4000 token budget"),
        ("--rank-by=authority", "Authority ranking"),
        ("--rank-by=hub", "Hub ranking"),
        ("--legend=compact", "Compact legend"),
        ("--legend=full", "Full legend"),
        ("--tree", "File inventory tree"),
        ("--dead-code", "Dead code detection"),
        ("--communities", "Community clustering"),
        ("--external-surface", "External API surface"),
        ("--deps", "File dependencies"),
        ("--hotspots", "Maintenance hotspots"),
        ("--cochange", "Shotgun surgery co-change mining"),
        ("--clones", "Clone detection"),
        ("-v", "Version banner"),
        ("--quality-delta", "Quality delta gate"),
        ("--test-gate", "Test reachability gate"),
        ("--map-diff --top-k=30", "Map diff biased teleportation"),
        ("--seams", "Integration seams detection"),
        ("--pr-context", "PR review context bundle"),
        (MERGE_SCOUT_CMD, "Cross-branch merge conflict prediction"),
        ("--biggest-first", "Posnett readability size ranking"),
        ("--nonlocal-state", "Reachable global mutable state"),
        ("--lint", "AST code smell scan"),
        ("--lint-catalog", "AST lint rule catalog"),
        (f"--at={target_file}:1", "Location definition resolver"),
        (f"--verify=calls({target_sym},{second_sym})", "Relationship claim verification"),
        ("--pack-task=search", "Task-scoped context pack"),
        (f'--graph-query=\'callers(name("{target_sym}"))\'', "Functional graph query DSL"),
        ("--exemplar=fn", "Best-in-class style exemplar"),
        ("--owners", "Git author ownership per file"),
        ("--doctor", "Environment and tool diagnostics"),
        ("--notes", "Persistent session gotchas"),
        ("--mermaid", "Mermaid diagram generation"),
        ("--html", "Interactive HTML visualizer export"),
        (f"--whereis={target_sym}", "Symbol cross-ref locator"),
        ("--flags", "Dark-content gate harvest"),
        ("--stray-content", "Cross-branch divergent content"),
        ("--dmm", "Delta Maintainability Model"),
        ("--quality-panel", "Unified 6-family quality panel"),
        ("--field-affinity", "Cache-line field co-access"),
        ("--context-ratio", "Local-reasoning context ratio"),
        ("--comment-coherence", "Doc-comment restatement coherence"),
        ('--plan-lanes=2 --task="refactor serialization"', "Parallel lane conflict planner"),
        ("--scan-skills=skills", "Skill security scanner"),
        ("--metrics", "Per-symbol complexity and amplification metrics"),
        ("--situ", "Situational awareness diff summary"),
        ("--handoff", "Session handoff summary"),
        (f"--around={target_sym}", "Ego-graph neighborhood"),
        (f"--callers={target_sym}", "Direct 1-hop callers"),
        (f"--callees={target_sym}", "Direct 1-hop callees"),
        (f"--uses={target_sym}", "All static use sites"),
        (f"--impact={target_sym}", "Transitive blast radius"),
        (f"--for={target_sym}", "Task-lens ranked inventory"),
        (f"--grep={target_sym}", "AST-annotated literal search"),
        (f"--slice={target_sym}", "Intra-procedural variable slice"),
        ("--lego=NonExistentTrait999", "Interface implementor lookup"),
        (f"--edit-check={target_sym}", "Post-edit contract check"),
        (f"--safe-delete={target_sym}", "Safe deletion pre-check"),
        *[(f"--affected={files}", "Test reachability on a changed set") for files in CHANGED_SETS],
        *[(f"--test-gate={files}", "Test obligations on a changed set") for files in CHANGED_SETS],
        *[(f"--situ={files}", "Situational awareness on a changed set") for files in CHANGED_SETS],
        *[(f"--edit-check={sym}", "Contract check for one symbol") for sym in GATE_SYMBOLS],
        *[(f"--safe-delete={sym}", "Safe-delete check for one symbol") for sym in GATE_SYMBOLS],
    ]


def parse_xml_dom(raw_text):
    clean = re.sub(r"<!--.*?-->", "", raw_text, flags=re.DOTALL).strip()
    if not clean:
        return None, "empty stdout"
    try:
        return ET.fromstring(clean), None
    except ET.ParseError as exc:
        return None, f"XML parse error: {exc}"


def required_attrs(el):
    return set(el.attrib) - OPTIONAL_ATTRS


def check_ranked_rows(tag, rip_rows, rw_rows):
    for ch_rip, ch_rw in zip(rip_rows, rw_rows):
        missing = required_attrs(ch_rip) - set(ch_rw.attrib)
        if missing:
            return f"ranked <{tag}> row missing required attrs {sorted(missing)}"
    return None


def row_gap(tag, ch_rip, rw_attrs, declared, used):
    need = required_attrs(ch_rip)
    if any(need <= have for have in rw_attrs):
        return None
    row = ch_rip.attrib.get("n", "")
    named = {attr for attr in need if f"{row}:{attr}" in declared}
    if not any(need - named <= have for have in rw_attrs):
        return f"child <{tag}> has no row with required attrs {sorted(need - named)}"
    used.update(f"{row}:{attr}" for attr in named)
    return None


def check_unranked_rows(tag, rip_rows, rw_rows, declared, used):
    rw_attrs = [set(ch.attrib) for ch in rw_rows]
    for ch_rip in rip_rows:
        err = row_gap(tag, ch_rip, rw_attrs, declared, used)
        if err:
            return err
    return None


def check_tag_rows(tag, root_rw, root_rip, ranked, declared, used):
    rip_rows = [ch for ch in root_rip if ch.tag == tag]
    rw_rows = [ch for ch in root_rw if ch.tag == tag]
    if ranked:
        return check_ranked_rows(tag, rip_rows, rw_rows)
    return check_unranked_rows(tag, rip_rows, rw_rows, declared, used)


def compare_xml_children(root_rw, root_rip, ranked, declared=frozenset(), used=None):
    """Check each upstream row has a same-tag row carrying its required attrs.

    `declared` holds `row:attr` omissions; an omitted attr is exempt from the check and recorded
    in `used` only when the row would otherwise fail, so unneeded omissions stay detectable."""
    used = set() if used is None else used
    for tag in sorted({ch.tag for ch in root_rip} & {ch.tag for ch in root_rw}):
        err = check_tag_rows(tag, root_rw, root_rip, ranked, declared, used)
        if err:
            return err
    return None


def omission_gaps(root_rw, root_rip, declared, used):
    """Exact omissions for an entry that declares `omits`: every upstream element that ours lacks
    and every attribute the row check exempted must be declared, and no declared omission may be
    unneeded."""
    attr_tokens = {token for token in declared if ":" in token}
    tag_tokens = declared - attr_tokens
    rip_tags = tag_counts(root_rip)
    missing_tags = {tag for tag in rip_tags if tag not in tag_counts(root_rw)}
    undeclared = sorted(missing_tags - tag_tokens)
    if undeclared:
        return f"upstream elements absent from ours and not declared in omits: {undeclared}"
    stale = sorted((tag_tokens - missing_tags) | (attr_tokens - used))
    if stale:
        return f"omits not needed by this output: {stale}; remove them"
    return None


def omit_declaration_errors(entry):
    """Structural rules for an `omits` list: smaller entries only, and each omitted name must be
    named in `why` so the reason is on record."""
    ident, omits = entry["id"], entry["omits"]
    if entry["kind"] != "smaller":
        return [f"ledger entry {ident} declares omits but kind is {entry['kind']}; only smaller entries may"]
    if not isinstance(omits, list) or not all(isinstance(token, str) and token for token in omits):
        return [f"ledger entry {ident} omits must be a list of non-empty strings"]
    errors = []
    if len(set(omits)) != len(omits):
        errors.append(f"ledger entry {ident} lists an omission twice")
    for token in omits:
        if not OMIT_TOKEN_RE.fullmatch(token):
            errors.append(f"ledger entry {ident} omission {token!r} must be 'row:attr' or 'element'")
            continue
        name = token.rsplit(":", 1)[-1]
        if not re.search(rf"(?<![A-Za-z0-9_-]){re.escape(name)}(?![A-Za-z0-9_-])", entry["why"]):
            errors.append(f"ledger entry {ident} omission {token!r} has no reason naming {name!r} in why")
    return errors


def check_declined_window_invariants(root_rw):
    rows = len(root_rw.findall("dc"))
    count = int(root_rw.attrib.get("declined_calls", "0"))
    if count < rows:
        return "impact declined_calls is smaller than its sample"
    if "dc_shown" in root_rw.attrib and int(root_rw.attrib["dc_shown"]) != rows:
        return "impact dc_shown differs from its sample"
    return None


def check_file_window_invariants(root_rw):
    if root_rw.tag != "impact":
        return None
    return check_declined_window_invariants(root_rw)


def check_caller_window_invariants(root_rw):
    if root_rw.tag != "callers":
        return None
    if root_rw.attrib.get("has_more") == "1" and not root_rw.attrib.get("uses_next"):
        return "callers capped rows missing uses_next"
    return None


def check_computed_metric_invariants(cmd, root_rw):
    if "--comment-coherence" in cmd:
        fns = root_rw.findall("fn")
        if len(fns) > 2 and len({f.attrib.get("words") for f in fns}) == 1:
            return "comment-coherence <fn> words= appears hardcoded"
    if "--context-ratio" in cmd:
        syms = root_rw.findall("s")
        if len(syms) > 2 and len({s.attrib.get("rtok") for s in syms}) == 1:
            return "context-ratio <s> rtok= appears hardcoded"
    return None


def check_xml_data_invariants(cmd, root_rw, root_rip):
    if "--dead-code" in cmd:
        c_rw = int(root_rw.attrib.get("count", "0"))
        c_rip = int(root_rip.attrib.get("count", "0"))
        if abs(c_rw - c_rip) > 50 and c_rw > c_rip * 5:
            return f"dead-code count mismatch: rw={c_rw} vs rip={c_rip}"
    if "--grep=" in cmd and any(not ch.attrib.get("p") for ch in root_rw.findall("f")):
        return "grep <f> has empty p attribute"
    return (check_file_window_invariants(root_rw)
            or check_caller_window_invariants(root_rw)
            or check_computed_metric_invariants(cmd, root_rw))


def root_identity_error(root_rw, root_rip):
    if root_rw.tag != root_rip.tag:
        return f"root tag mismatch: rw=<{root_rw.tag}> vs rip=<{root_rip.tag}>"
    schema_rw = root_rw.attrib.get("schema", "")
    schema_rip = root_rip.attrib.get("schema", "")
    if (schema_rw or schema_rip) and (
        not schema_rw.startswith("ripwire-rs.")
        or schema_rw.replace("ripwire-rs.", "ripwire.", 1) != schema_rip
    ):
        return f"schema mismatch: rw={schema_rw} vs rip={schema_rip}"
    return None


def root_attr_error(root_rw, root_rip, allow_extra_root):
    req_rip = set(root_rip.attrib.keys()) - OPTIONAL_ATTRS
    req_rw = set(root_rw.attrib.keys()) - OPTIONAL_ATTRS
    if allow_extra_root:
        missing_root = sorted(req_rip - req_rw)
        if missing_root:
            return f"root attr keys missing upstream attrs {missing_root}"
    elif req_rw != req_rip:
        missing_root = sorted(req_rip - req_rw)
        extra_root = sorted(req_rw - req_rip)
        return f"root attr keys mismatch: missing={missing_root} extra={extra_root}"
    return None


def budget_error(cmd, out_rw, out_rip):
    if "--token-budget=2000" in cmd and len(out_rw) > max(len(out_rip) * 3, 10000):
        return f"budget overrun: rw={len(out_rw)}B vs rip={len(out_rip)}B"
    return None


def check_xml_parity(cmd, out_rw, out_rip, allow_extra_root=False, omits=None):
    root_rw, err_rw = parse_xml_dom(out_rw)
    if err_rw:
        return f"ripwire-rs {err_rw}"
    root_rip, err_rip = parse_xml_dom(out_rip)
    if err_rip:
        return f"ripwire {err_rip}"
    err = (
        root_identity_error(root_rw, root_rip)
        or root_attr_error(root_rw, root_rip, allow_extra_root)
        or budget_error(cmd, out_rw, out_rip)
        or check_xml_data_invariants(cmd, root_rw, root_rip)
    )
    if err:
        return err
    declared = set(omits or ())
    used = set()
    err = compare_xml_children(root_rw, root_rip, is_ranked(cmd), declared, used)
    if err or omits is None:
        return err
    return omission_gaps(root_rw, root_rip, declared, used)


def check_plan_lanes_json(out_rw, out_rip):
    try:
        j_rw = json.loads(out_rw.strip())
        j_rip = json.loads(out_rip.strip())
    except Exception as exc:
        return f"plan-lanes JSON error: {exc}"
    if len(out_rw.strip().splitlines()) != 1:
        return "plan-lanes must be 1-line JSON"
    missing = sorted(set(j_rip.keys()) - set(j_rw.keys()) - {"warnings", "notes_degraded"})
    return f"plan-lanes missing keys {missing}" if missing else None


def check_situ_xml(output):
    root, error = parse_xml_dom(output)
    if error:
        return f"situ {error}"
    if root.tag != "situ" or root.attrib.get("schema") != "ripwire-rs.situ/v1":
        return "situ requires <situ schema=\"ripwire-rs.situ/v1\">"
    summary = root.find("summary")
    if summary is None or not {"files", "blast_radius"} <= summary.attrib.keys():
        return "situ summary missing files or blast_radius"
    return None


def check_non_xml_parity(cmd, out_rw, out_rip):
    if cmd == "-v":
        return None if ("ripwire" in out_rw or "rw" in out_rw) else "missing version string"
    if "--mermaid" in cmd:
        return None if "flowchart" in out_rw else "missing flowchart"
    if "--html" in cmd:
        return None if ("<!DOCTYPE html>" in out_rw and "const NODES" in out_rw) else "missing html"
    if "--situ" in cmd:
        return check_situ_xml(out_rw)
    if "--plan-lanes" in cmd:
        return check_plan_lanes_json(out_rw, out_rip)
    return "unhandled text command"


def is_text_cmd(cmd):
    base = cmd.split("=", 1)[0]
    return cmd == "-v" or base in ("--mermaid", "--html", "--situ") or "--plan-lanes" in cmd


def is_ranked(cmd):
    return cmd.split("=", 1)[0] in RANKED_FLAGS


def check_text_timeout(cmd, output):
    if cmd.split("=", 1)[0] == "--situ":
        return check_situ_xml(output)
    return None if output.strip() else "empty text output"


def check_xml_timeout(output):
    root, error = parse_xml_dom(output)
    return error if error else (None if root.attrib.get("schema") else "missing schema")


Outcome = collections.namedtuple("Outcome", "reason rw rip text")


def full_window_outputs(rw_bin, rip_bin, repo, cmd, out_rw, out_rip):
    """Both outputs at the full window when the default one is a paged list.

    A default window is a byte budget, so what it shows says nothing about which attributes a row
    can carry; structure is judged on the full set and default sizes by `make size`. A budget
    command keeps its own windows, and any failure to run the full set keeps the default outputs.
    """
    root, err = parse_xml_dom(out_rw)
    if err or not is_paged(root) or FULL_ROWS_FLAG in cmd or "--token-budget" in cmd:
        return out_rw, out_rip
    args = shlex.split(cmd) + [FULL_ROWS_FLAG]
    try:
        full_rw = subprocess.run([rw_bin, repo] + args, capture_output=True, text=True, timeout=20)
        full_rip = subprocess.run([rip_bin, repo] + args, capture_output=True, text=True, timeout=FULL_TIMEOUT)
    except subprocess.TimeoutExpired:
        return out_rw, out_rip
    if full_rw.returncode != 0 or full_rip.returncode != 0:
        return out_rw, out_rip
    return full_rw.stdout, full_rip.stdout


def upstream_argv(cmd):
    """Upstream ripwire spells the token budget `--max-tokens`; every other flag is shared."""
    return [arg.replace("--token-budget", "--max-tokens", 1) for arg in shlex.split(cmd)]


def command_argv(rw_bin, rip_bin, repo, cmd):
    if cmd == "-v":
        return [rw_bin, "-v"], [rip_bin, "-v"]
    return [rw_bin, repo] + shlex.split(cmd), [rip_bin, repo] + upstream_argv(cmd)


def timeout_outcome(cmd, text, p_rw):
    print(
        f"note: upstream {cmd or 'default map'} exceeded {UPSTREAM_TIMEOUT}s; not compared "
        "(raise PARITY_UPSTREAM_TIMEOUT to measure it)"
    )
    if text:
        return Outcome(check_text_timeout(cmd, p_rw.stdout), None, None, text)
    return Outcome(check_xml_timeout(p_rw.stdout), None, None, text)


def exit_mismatch_tolerated(repo, cmd, p_rw, p_rip):
    if p_rw.returncode != 0 and p_rip.returncode != 0:
        return True
    if any(flag in cmd for flag in ("--quality-delta", "--test-gate", "--scan-skills")):
        return True
    return repo != "." and any(k in cmd for k in ("--slice=", "--edit-check=", "--at=", "--doctor"))


def exit_code_outcome(repo, cmd, text, p_rw, p_rip):
    """Outcome when the two exit codes disagree, or None to keep judging the outputs."""
    if p_rw.returncode == p_rip.returncode:
        return None
    if not exit_mismatch_tolerated(repo, cmd, p_rw, p_rip):
        return Outcome(f"exit code mismatch: rw={p_rw.returncode} vs rip={p_rip.returncode}", p_rw.stdout, p_rip.stdout, text)
    if not p_rw.stdout.strip() or not p_rip.stdout.strip():
        return Outcome(None, None, None, text)
    return None


def compare_outputs(rw_bin, rip_bin, repo, cmd, text, p_rw, p_rip, allow_extra_root, omits):
    if text:
        return check_non_xml_parity(cmd, p_rw.stdout, p_rip.stdout)
    rw_out, rip_out = full_window_outputs(rw_bin, rip_bin, repo, cmd, p_rw.stdout, p_rip.stdout)
    return check_xml_parity(cmd, rw_out, rip_out, allow_extra_root, omits)


def run_one_check(rw_bin, rip_bin, repo, cmd, allow_extra_root=False, omits=None):
    text = is_text_cmd(cmd)
    c_rw, c_rip = command_argv(rw_bin, rip_bin, repo, cmd)
    p_rw = subprocess.run(c_rw, capture_output=True, text=True, timeout=20)
    try:
        p_rip = subprocess.run(c_rip, capture_output=True, text=True, timeout=UPSTREAM_TIMEOUT)
    except subprocess.TimeoutExpired:
        return timeout_outcome(cmd, text, p_rw)
    early = exit_code_outcome(repo, cmd, text, p_rw, p_rip)
    if early is not None:
        return early
    if p_rw.returncode != 0 and not p_rip.stdout.strip():
        return Outcome(None, None, None, text)
    reason = compare_outputs(rw_bin, rip_bin, repo, cmd, text, p_rw, p_rip, allow_extra_root, omits)
    return Outcome(reason, p_rw.stdout, p_rip.stdout, text)


def tag_counts(root):
    return collections.Counter(el.tag for el in root.iter() if el is not root)


def tag_divergence(root_rw, root_rip):
    c_rw, c_rip = tag_counts(root_rw), tag_counts(root_rip)
    return {t: (c_rw[t], c_rip[t]) for t in set(c_rw) | set(c_rip) if c_rw[t] != c_rip[t]}


def facts(root):
    return sum(1 + len(el.attrib) for el in root.iter())


def divergence_roots(outcome):
    if outcome.text or outcome.rw is None:
        return None
    root_rw, err_rw = parse_xml_dom(outcome.rw)
    root_rip, err_rip = parse_xml_dom(outcome.rip)
    if err_rw or err_rip:
        return None
    return root_rw, root_rip


def entry_key(entry):
    case = entry["case"]
    if isinstance(case, list):
        case = ",".join(case)
    return f"{entry['command']}={case}" if case else entry["command"]


def ledger_entry_errors(entry, ids, keys):
    ident = entry.get("id", "?")
    missing = [f for f in LEDGER_FIELDS if f not in entry]
    if missing:
        return [f"ledger entry {ident} missing {missing}"]
    errors = []
    if "omits" in entry:
        errors.extend(omit_declaration_errors(entry))
    if entry["kind"] not in KINDS:
        errors.append(f"ledger entry {ident} has unknown kind {entry['kind']!r}")
    if ident in ids:
        errors.append(f"duplicate ledger id {ident}")
    ids.add(ident)
    if "files" not in entry:
        key = entry_key(entry)
        if key in keys:
            errors.append(f"duplicate ledger command {key}")
        keys.add(key)
    return errors


def load_ledger():
    with LEDGER.open("rb") as fh:
        data = tomllib.load(fh)
    entries = data.get("divergence", [])
    errors = []
    if not isinstance(data.get("defect_budget"), int):
        errors.append("ledger header must set defect_budget to the number of defect entries")
    ids, keys = set(), set()
    for entry in entries:
        errors.extend(ledger_entry_errors(entry, ids, keys))
    return entries, data.get("defect_budget", 0), errors


def materialize_fixture(files):
    root = Path(tempfile.mkdtemp(prefix="ripwire-parity-")).resolve()
    try:
        for rel, content in files.items():
            target = (root / rel).resolve()
            if root not in target.parents:
                raise ValueError(f"fixture path escapes its root: {rel}")
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_text(content, encoding="utf-8")
        git = ["git", "-C", str(root), "-c", "user.name=parity", "-c", "user.email=parity@localhost"]
        subprocess.run(["git", "-C", str(root), "init", "-q"], check=True)
        subprocess.run(git + ["add", "-A"], check=True)
        subprocess.run(git + ["commit", "-q", "-m", "fixture"], check=True)
    except Exception:
        shutil.rmtree(root, ignore_errors=True)
        raise
    return str(root)


def check_defect_evidence(evidence):
    match = EVIDENCE_RE.fullmatch(evidence)
    if match is None:
        return f"fixes-upstream-defect evidence must be tests/<mod>/<file>.rs::test_name, got {evidence!r}"
    rel, name = match.group(1), match.group(2)
    path = ROOT / rel
    if not path.is_file():
        return f"regression test file {rel} does not exist"
    if not re.search(rf"fn\s+{re.escape(name)}\s*\(", path.read_text(encoding="utf-8")):
        return f"fn {name} not found in {rel}"
    return None


def measured_verdict(entry, b_rw, b_rip, f_rw, f_rip):
    d_rw, d_rip = f_rw / max(b_rw, 1), f_rip / max(b_rip, 1)
    print(
        f"ledger {entry['id']} [{entry['kind']}]: bytes rw={b_rw} rip={b_rip}; "
        f"facts rw={f_rw} rip={f_rip}; density rw={d_rw:.5f} rip={d_rip:.5f}"
    )
    kind = entry["kind"]
    if kind == "smaller":
        return None if b_rw <= b_rip else f"smaller: rw bytes {b_rw} exceed rip bytes {b_rip}"
    if kind == "richer":
        ok = f_rw > f_rip and d_rw >= d_rip
        return None if ok else "richer: needs more facts and at least equal facts-per-byte than upstream"
    return check_defect_evidence(entry["evidence"])


def check_entry(entry, outcome):
    root_rw, root_rip = divergence_roots(outcome)
    return measured_verdict(
        entry, len(outcome.rw.encode()), len(outcome.rip.encode()), facts(root_rw), facts(root_rip)
    )


def xml_facts(text):
    root, err = parse_xml_dom(text)
    return 0 if err else facts(root)


def check_exit_entry(entry, outcome):
    return measured_verdict(
        entry, len(outcome.rw.encode()), len(outcome.rip.encode()), xml_facts(outcome.rw), xml_facts(outcome.rip)
    )


def describe_divergence(diff):
    return ", ".join(f"<{t}> rw={a} rip={b}" for t, (a, b) in sorted(diff.items()))


def is_paged(root):
    return any(attr in root.attrib for attr in PAGING_ATTRS)


def row_value(key, value):
    return re.sub(r":\d+$", "", value) if key == "p" else value


def row_groups(root):
    groups = collections.defaultdict(list)
    for el in root.iter():
        if el is not root and any(k in el.attrib for k in ROW_KEYS):
            groups[el.tag].append(tuple((k, row_value(k, el.attrib[k])) for k in ROW_KEYS if k in el.attrib))
    return groups


def size_verdict(out_rw, out_rip, root_rw, root_rip):
    b_rw, b_rip = len(out_rw.encode()), len(out_rip.encode())
    if b_rw <= b_rip:
        return None
    f_rw, f_rip = facts(root_rw), facts(root_rip)
    if f_rw > f_rip and f_rw / b_rw >= f_rip / b_rip:
        return None
    return f"paged output {b_rw}B exceeds upstream {b_rip}B without richer density (facts {f_rw} vs {f_rip})"


def is_subsequence(part, whole):
    it = iter(whole)
    return all(any(x == y for y in it) for x in part)


def run_full_rows(rw_bin, rip_bin, repo, cmd):
    full_rw = subprocess.run(
        [rw_bin, repo] + shlex.split(cmd) + [FULL_ROWS_FLAG], capture_output=True, text=True, timeout=20
    )
    full_rip = subprocess.run(
        [rip_bin, repo] + upstream_argv(cmd) + [FULL_ROWS_FLAG], capture_output=True, text=True, timeout=FULL_TIMEOUT
    )
    return full_rw.stdout, full_rip.stdout


def paged_row_error(root_rw, rows_full_rw, rows_full_rip):
    for tag, keys in sorted(row_groups(root_rw).items()):
        if not is_subsequence(keys, rows_full_rw.get(tag, [])):
            return f"paged <{tag}> rows are out of our ranking order"
        extra = sorted(set(keys) - set(rows_full_rip.get(tag, [])))
        if extra:
            return f"paged <{tag}> row absent from upstream's full set: {extra[0]}"
    return None


def paged_count_error(root_rw, root_rip):
    for attr in ("total", "count"):
        if attr in root_rw.attrib and attr in root_rip.attrib and root_rw.attrib[attr] != root_rip.attrib[attr]:
            return f"paged {attr} mismatch: rw={root_rw.attrib[attr]} vs rip={root_rip.attrib[attr]}"
    return None


def paged_offset_error(root_rw):
    if "next_offset" in root_rw.attrib and "shown" in root_rw.attrib:
        expected = int(root_rw.attrib.get("offset", "0")) + int(root_rw.attrib["shown"])
        if int(root_rw.attrib["next_offset"]) != expected:
            return "paged next_offset does not follow the shown rows"
    return None


def paged_verdict(rw_bin, rip_bin, repo, cmd, out_rw, out_rip, root_rw, root_rip):
    try:
        full_out_rw, full_out_rip = run_full_rows(rw_bin, rip_bin, repo, cmd)
    except subprocess.TimeoutExpired:
        return "paged list: full row set timed out"
    root_full_rw, err_rw = parse_xml_dom(full_out_rw)
    root_full_rip, err_rip = parse_xml_dom(full_out_rip)
    if err_rw or err_rip:
        return "paged list: full row set not comparable"
    return (
        paged_row_error(root_rw, row_groups(root_full_rw), row_groups(root_full_rip))
        or paged_count_error(root_rw, root_rip)
        or paged_offset_error(root_rw)
        or size_verdict(out_rw, out_rip, root_rw, root_rip)
    )


def generic_covers(rw_bin, rip_bin, repo, cmd, outcome, roots):
    return is_paged(roots[0]) and paged_verdict(rw_bin, rip_bin, repo, cmd, outcome.rw, outcome.rip, *roots) is None


def defect_judgement(rw_bin, rip_bin, repo, cmd, outcome, entry, roots, diff):
    ident = entry["id"]
    if not outcome.reason and not diff:
        return f"ledger entry {ident} no longer diverges; remove it", None
    if not outcome.reason and generic_covers(rw_bin, rip_bin, repo, cmd, outcome, roots):
        return f"ledger entry {ident} is covered by the generic paged-list rule; remove it", None
    return None, f"{ident}: {entry['why']}"


def stale_message(entry, why):
    return f"ledger entry {entry['id']} {why}" if entry else None


def paged_judgement(rw_bin, rip_bin, repo, cmd, outcome, entry, roots):
    reason = paged_verdict(rw_bin, rip_bin, repo, cmd, outcome.rw, outcome.rip, *roots)
    if reason is None:
        return True, stale_message(entry, "is covered by the generic paged-list rule; remove it")
    return entry is None, reason


def measured_judgement(rw_bin, rip_bin, repo, cmd, outcome, entry, roots, diff):
    if roots is None:
        return stale_message(entry, "cannot be measured: output not comparable"), None
    declared_omits = bool(entry and entry.get("omits"))
    if not diff and not declared_omits:
        return stale_message(entry, "no longer diverges; remove it"), None
    if is_paged(roots[0]):
        handled, verdict = paged_judgement(rw_bin, rip_bin, repo, cmd, outcome, entry, roots)
        if handled:
            return verdict, None
    if entry is None:
        return f"unrecorded difference {describe_divergence(diff)}; record it in scripts/parity-divergences.toml", None
    return check_entry(entry, outcome), None


def judge(rw_bin, rip_bin, repo, cmd, outcome, entry):
    roots = divergence_roots(outcome)
    diff = tag_divergence(*roots) if roots else {}
    if entry is not None and entry["kind"] == "defect":
        if outcome.reason:
            return outcome.reason, None
        return defect_judgement(rw_bin, rip_bin, repo, cmd, outcome, entry, roots, diff)
    if outcome.reason and (entry is None or not outcome.reason.startswith("exit code mismatch")):
        return outcome.reason, None
    if outcome.reason:
        return check_exit_entry(entry, outcome), None
    return measured_judgement(rw_bin, rip_bin, repo, cmd, outcome, entry, roots, diff)



def check_one(rw_bin, rip_bin, where, cmd, desc, entry, run_cmd=None):
    kind_allows_extra = entry is not None and entry["kind"] in ("richer", "smaller")
    omits = entry.get("omits") if entry is not None else None
    outcome = run_one_check(rw_bin, rip_bin, where, run_cmd or cmd, kind_allows_extra, omits)
    reason, warning = judge(rw_bin, rip_bin, where, run_cmd or cmd, outcome, entry)
    if warning is not None:
        WARNINGS.append(warning)
        print(f"::warning::{warning} ({cmd or '(default)'})", file=sys.stderr)
    if reason is None:
        return 0
    report_error(f"Parity failed on {where}: {cmd or '(default)'} - {desc} ({reason})")
    return 1


def load_checked_ledger():
    entries, budget, errors = load_ledger()
    defect_count = sum(1 for entry in entries if entry["kind"] == "defect")
    if defect_count != budget:
        errors.append(f"ledger has {defect_count} defect entries but defect_budget is {budget}; set defect_budget to the entry count, and review any increase")
    if errors:
        for err in errors:
            report_error(err)
        sys.exit(1)
    return entries, budget


def run_builtin_checks(rw_bin, rip_bin, repo, merge_ref, commands, tree_entries):
    failures = 0
    for cmd, desc in commands:
        run_cmd = cmd
        if cmd == MERGE_SCOUT_CMD:
            if merge_ref == "HEAD":
                print(f"note: skipped {cmd} ({desc}): none of {', '.join(MERGE_BASE_CANDIDATES)} resolves on this checkout, so there is no base branch to scout against")
                continue
            run_cmd = f"--merge-scout={merge_ref}"
        failures += check_one(rw_bin, rip_bin, repo, cmd, desc, tree_entries.get(cmd), run_cmd)
    return failures


def run_ledger_only_checks(rw_bin, rip_bin, repo, builtin, tree_entries):
    failures = 0
    for key, entry in tree_entries.items():
        if key not in builtin:
            failures += check_one(rw_bin, rip_bin, repo, key, f"ledger {entry['id']}", entry)
    return failures


def run_fixture_checks(rw_bin, rip_bin, fixture_entries):
    failures = 0
    for entry in fixture_entries:
        fixture = materialize_fixture(entry["files"])
        try:
            failures += check_one(rw_bin, rip_bin, fixture, entry_key(entry), f"ledger {entry['id']}", entry)
        finally:
            shutil.rmtree(fixture, ignore_errors=True)
    return failures


def main():
    repo = sys.argv[1] if len(sys.argv) > 1 else "."
    if not os.path.exists(repo):
        report_error(f"Repository path '{repo}' does not exist")
        sys.exit(1)
    entries, budget = load_checked_ledger()
    defects = [e for e in entries if e["kind"] == "defect"]
    rw_bin, rip_bin = find_binaries()
    merge_ref = detect_merge_ref(repo)
    target_file, target_sym, second_sym = discover_repo_targets(rw_bin, repo)
    commands = build_command_list(target_file, target_sym, second_sym)
    builtin = {cmd for cmd, _ in commands}
    tree_entries = {entry_key(e): e for e in entries if "files" not in e}
    fixture_entries = [e for e in entries if "files" in e]
    failures = run_builtin_checks(rw_bin, rip_bin, repo, merge_ref, commands, tree_entries)
    failures += run_ledger_only_checks(rw_bin, rip_bin, repo, builtin, tree_entries)
    failures += run_fixture_checks(rw_bin, rip_bin, fixture_entries)
    print(f"parity: {len(WARNINGS)} defect warnings, {len(defects)} defect entries (budget {budget})")
    sys.exit(0 if failures == 0 else 1)


if __name__ == "__main__":
    main()
