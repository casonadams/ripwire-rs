#!/usr/bin/env python3
import json
import subprocess
import sys


MAX_COGNITIVE = 15


def escape(value):
    return str(value).replace("%", "%25").replace("\r", "%0D").replace("\n", "%0A")


def report_error(message, path=None, line=None):
    properties = ""
    if path is not None:
        file = escape(path).replace(",", "%2C").replace(":", "%3A")
        properties = f" file={file},line={line}"
    print(f"::error{properties}::{escape(message)}", file=sys.stderr)


def report_violations(data):
    count = 0
    for file in data["files"]:
        for function in file["functions"]:
            score = function["cognitive"]
            if score > MAX_COGNITIVE:
                count += 1
                report_error(
                    f"{function['name']}: cognitive complexity {score} exceeds {MAX_COGNITIVE}",
                    file["path"], function["line"],
                )
    return count


def main():
    paths = sys.argv[1:] or ["."]
    command = ["cccc", "--max-cognitive", str(MAX_COGNITIVE), "--lang", "rust,python,javascript", *paths]
    try:
        result = subprocess.run(command, capture_output=True, text=True)
    except FileNotFoundError:
        report_error("cccc not found. Install with: cargo install cccc-cli")
        return 1
    if result.stderr.strip():
        report_error(result.stderr.strip())
    try:
        violations = report_violations(json.loads(result.stdout))
    except (ValueError, KeyError, TypeError) as error:
        report_error(f"Invalid cccc JSON report: {error}")
        return 1
    if result.returncode and not violations and not result.stderr.strip():
        report_error(f"cccc exited with status {result.returncode}")
    return result.returncode or int(violations > 0)


if __name__ == "__main__":
    sys.exit(main())
