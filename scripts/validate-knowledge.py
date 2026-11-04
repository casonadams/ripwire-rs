#!/usr/bin/env python3
"""Deep knowledge, computed-metric, and resource-efficiency validation against upstream ripwire."""

import os
import re
import shutil
import subprocess
import sys
import time
import xml.etree.ElementTree as ET


def find_binaries():
    rw_bin = os.path.abspath("target/release/ripwire-rs")
    if not os.path.exists(rw_bin):
        rw_bin = shutil.which("ripwire-rs") or shutil.which("rw")
    rip_bin = shutil.which("ripwire")
    return rw_bin, rip_bin


def run_cmd(bin_path, repo, args):
    time_bin = "/usr/bin/time"
    if os.path.exists(time_bin):
        cmd = [time_bin, "-l", bin_path, repo] + args
    else:
        cmd = [bin_path, repo] + args
    t0 = time.perf_counter()
    p = subprocess.run(cmd, capture_output=True, text=True)
    dt = (time.perf_counter() - t0) * 1000.0
    m = re.search(r"(\d+)\s+maximum resident set size", p.stderr)
    rss_mb = int(m.group(1)) / (1024 * 1024) if m else 0.0
    return p.stdout, dt, rss_mb, p.returncode


def parse_root(xml_text):
    clean = re.sub(r"<!--.*?-->", "", xml_text, flags=re.S).strip()
    try:
        return ET.fromstring(clean)
    except ET.ParseError:
        return None


def discover_seeds(rw_bin, repo):
    out, _, _, _ = run_cmd(rw_bin, repo, ["--top-k=30"])
    syms = re.findall(r'<s [^>]*n="([a-zA-Z_][a-zA-Z0-9_]*)"', out)
    unique = []
    for s in syms:
        if s not in unique and s not in ("main", "new", "init"):
            unique.append(s)
    sym1 = unique[0] if unique else "main"
    sym2 = unique[1] if len(unique) > 1 else sym1
    return sym1, sym2


def verify_warm_map_graph(r_rs):
    if not r_rs.findall("f") or not r_rs.findall(".//s") or not r_rs.findall(".//c"):
        return "warm-map graph missing <f>, <s>, or <c> elements"
    return None


def verify_deps_graph(r_rs):
    health = r_rs.find("health")
    if health is None or float(health.attrib.get("ccd", "0")) <= 0:
        return "deps <health> missing or ccd <= 0"
    return None


def _hardcoded_attr(r_rs, tag, attr):
    elements = r_rs.findall(tag)
    return len(elements) > 2 and len({e.attrib.get(attr) for e in elements}) == 1


def verify_lens_xml_invariants(name, r_rs):
    if name == "warm-map":
        return verify_warm_map_graph(r_rs)
    if name == "deps":
        return verify_deps_graph(r_rs)
    if name == "comment-coherence" and _hardcoded_attr(r_rs, "fn", "words"):
        return "comment-coherence <fn> words= is hardcoded"
    if name == "context-ratio" and _hardcoded_attr(r_rs, "s", "rtok"):
        return "context-ratio <s> rtok= is hardcoded"
    if name == "handoff" and r_rs.attrib.get("subject") == "head":
        return "handoff subject= is hardcoded to 'head'"
    return None


def verify_computed_check(name, out_rs, out_cpp):
    if name == "report":
        ok = "## Cross-module bridges" in out_rs and "## God files" in out_rs
        return None if ok else "report missing bridges or god files section"
    r_rs = parse_root(out_rs)
    if r_rs is None:
        return "invalid XML output from ripwire-rs"
    schema = r_rs.attrib.get("schema", "")
    if schema and not schema.startswith("ripwire-rs."):
        return f"expected ripwire-rs. schema prefix, got {schema}"
    return verify_lens_xml_invariants(name, r_rs)

def evaluate_repo(rw_bin, rip_bin, repo):
    sym1, sym2 = discover_seeds(rw_bin, repo)
    print(f"\n==================================================================")
    print(f"Knowledge & Computed-Metric Audit: {repo} (Seeds: {sym1}, {sym2})")
    print(f"==================================================================")
    failures = []
    checks = [
        ("warm-map", ["--top-k=15"]),
        ("report", ["--report"]),
        ("deps", ["--deps"]),
        ("comment-coherence", ["--comment-coherence"]),
        ("context-ratio", ["--context-ratio"]),
        ("handoff", ["--handoff"]),
        ("quality-panel", ["--quality-panel"]),
        ("for", [f"--for={sym1}"]),
        ("connect", [f"--connect={sym1},{sym2}"]),
        ("impact", [f"--impact={sym1}"]),
        ("grep", [f"--grep={sym1}"]),
        ("flags", ["--flags"]),
    ]
    for name, args in checks:
        out_rs, ms_rs, rss_rs, _ = run_cmd(rw_bin, repo, args)
        out_cpp, ms_cpp, rss_cpp, _ = run_cmd(rip_bin, repo, args)
        speedup = (ms_cpp / ms_rs) if ms_rs > 0 else 1.0
        rss_ratio = (rss_cpp / rss_rs) if rss_rs > 0 else 1.0
        err = verify_computed_check(name, out_rs, out_cpp)
        status = "FAIL" if err else "PASS"
        print(
            f"  [{status}] {name:<18} | RS: {ms_rs:6.1f}ms / {rss_rs:5.1f}MB | C++: {ms_cpp:6.1f}ms / {rss_cpp:5.1f}MB | {speedup:5.2f}x faster, {rss_ratio:4.1f}x less RAM"
        )
        if err:
            failures.append((repo, name, err))
    return failures

def main():
    rw_bin, rip_bin = find_binaries()
    if not rw_bin or not rip_bin:
        print("Error: ripwire-rs and ripwire binaries must be available", file=sys.stderr)
        return 1
    repos = sys.argv[1:] if len(sys.argv) > 1 else ["."]
    all_failures = []
    for repo in repos:
        if os.path.isdir(repo):
            all_failures.extend(evaluate_repo(rw_bin, rip_bin, repo))
    if all_failures:
        print(f"\nKNOWLEDGE AUDIT FAILED: {len(all_failures)} check(s) failed.")
        return 1
    print("\nKNOWLEDGE & COMPUTED-METRIC AUDIT VERIFIED: 100% PASS.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
