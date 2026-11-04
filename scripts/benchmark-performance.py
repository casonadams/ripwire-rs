#!/usr/bin/env python3
import os
import re
import shutil
import subprocess
import sys


def locate_bench_binaries():
    rs_cands = [os.path.abspath("target/release/ripwire-rs"), shutil.which("ripwire-rs")]
    cpp_cands = [os.path.expanduser("~/.local/bin/ripwire"), shutil.which("ripwire")]
    rs_bin = next((p for p in rs_cands if p and os.path.exists(p)), None)
    cpp_bin = next((p for p in cpp_cands if p and os.path.exists(p)), None)
    if not rs_bin or not cpp_bin:
        sys.exit("Error: ripwire-rs or ripwire binary not found.")
    return rs_bin, cpp_bin


def measure_process(cmd):
    try:
        proc = subprocess.run(["/usr/bin/time", "-l"] + cmd, capture_output=True, text=True, timeout=8)
    except subprocess.TimeoutExpired:
        return None, 8.0, 8.0, 500.0
    err = proc.stderr
    m_time = re.search(r"([0-9.]+)\s+real\s+([0-9.]+)\s+user\s+([0-9.]+)\s+sys", err)
    if m_time:
        real_sec = float(m_time.group(1))
        cpu_sec = float(m_time.group(2)) + float(m_time.group(3))
    else:
        m_real = re.search(r"([0-9.]+)\s+real", err)
        real_sec = float(m_real.group(1)) if m_real else 0.0
        cpu_sec = real_sec
    m_rss = re.search(r"([0-9]+)\s+maximum resident set size", err)
    max_rss_mb = (int(m_rss.group(1)) / (1024 * 1024)) if m_rss else 0.0
    return proc, real_sec, cpu_sec, max_rss_mb


def discover_seed_symbols(rs_bin, repo):
    proc = subprocess.run([rs_bin, repo], capture_output=True, text=True)
    syms = re.findall(r'<s t="(?:fn|method)" n="([a-zA-Z0-9_]+)"', proc.stdout)
    if not syms:
        syms = re.findall(r'<s [^>]*n="([a-zA-Z0-9_]+)"', proc.stdout)
    uniq = []
    for candidate in syms:
        if candidate not in uniq and len(candidate) > 2:
            uniq.append(candidate)
    if not uniq:
        return "main", "init"
    if len(uniq) == 1:
        return uniq[0], uniq[0]
    return uniq[0], uniq[1]


def build_bench_suite(primary_sym, secondary_sym):
    return [
        ("Map (Warm)", []),
        ("Map (Cold)", ["--no-cache"]),
        ("Tree", ["--tree"]),
        ("Dead Code", ["--dead-code"]),
        ("Communities", ["--communities"]),
        ("Deps", ["--deps"]),
        ("Callers", [f"--callers={primary_sym}"]),
        ("Callees", [f"--callees={primary_sym}"]),
        ("Impact", [f"--impact={primary_sym}"]),
        ("Uses", [f"--uses={primary_sym}"]),
        ("For Task", [f"--for=explore and inspect {primary_sym}"]),
        ("Grep", [f"--grep={primary_sym}"]),
        ("Connect", [f"--connect={primary_sym},{secondary_sym}"]),
        ("Path", [f"--path-between={primary_sym},{secondary_sym}"]),
        ("Whereis", [f"--whereis={primary_sym}"]),
        ("Doc Drift", ["--doc-drift"]),
        ("Flags", ["--flags"]),
        ("Stray Content", ["--stray-content"]),
        ("Owners", ["--owners"]),
        ("AST Clones", ["--clones"]),
        ("Cochange", ["--cochange"]),
        ("Hotspots", ["--hotspots"]),
        ("Quality Delta", ["--quality-delta"]),
        ("Quality Panel", ["--quality-panel"]),
        ("Context Ratio", ["--context-ratio"]),
    ]


def main():
    repo = sys.argv[1] if len(sys.argv) > 1 else "."
    if not os.path.exists(repo):
        print(f"Error: repository path '{repo}' does not exist", file=sys.stderr)
        sys.exit(1)
    rs_bin, cpp_bin = locate_bench_binaries()
    primary_sym, secondary_sym = discover_seed_symbols(rs_bin, repo)
    subprocess.run([rs_bin, repo], capture_output=True)
    subprocess.run([cpp_bin, repo], capture_output=True)
    commands = build_bench_suite(primary_sym, secondary_sym)
    print("=" * 108)
    print(f"Performance & Resource Benchmark (Wall, CPU, RSS): {os.path.abspath(repo)}")
    print(f"Symbols: {primary_sym}, {secondary_sym}")
    print(f"Rust: {rs_bin} | C++: {cpp_bin}")
    print("=" * 108)
    print("Operation        |  RS Wall | C++ Wall | Speedup |   RS CPU |  C++ CPU |   RS RSS |  C++ RSS |  RSS Delta")
    print("-" * 108)
    fmt = "{:<16} | {:>7.3f}s | {:>7.3f}s | {:>6.2f}x | {:>7.3f}s | {:>7.3f}s | {:>6.1f}MB | {:>6.1f}MB | {:>10}"
    for name, args in commands:
        _, rs_w, rs_c, rs_m = measure_process([rs_bin, repo] + args)
        _, cpp_w, cpp_c, cpp_m = measure_process([cpp_bin, repo] + args)
        speedup = cpp_w / rs_w if rs_w > 0 else 1.0
        mem_delta = f"{((rs_m - cpp_m) / cpp_m * 100):+.1f}%" if cpp_m > 0 else "0.0%"
        print(fmt.format(name, rs_w, cpp_w, speedup, rs_c, cpp_c, rs_m, cpp_m, mem_delta))
    print("=" * 108)


if __name__ == "__main__":
    main()
