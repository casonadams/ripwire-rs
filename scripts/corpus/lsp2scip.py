#!/usr/bin/env python3
"""Turn a language server's go-to-definition answers into a SCIP index for ripwire-rs --eval=scip.

The bridge speaks LSP over stdio to SERVER, enumerates call-site candidates with a conservative
`name(` scan of every source file, asks textDocument/definition about each one, and writes a SCIP
index that documents every source file. A candidate with exactly one definition inside the checkout
becomes a reference occurrence pinned to that definition (and the definition is written as a
definition occurrence); one definition outside the checkout becomes an external reference; no answer,
or several distinct answers, writes nothing for that candidate.

Recall is therefore measured over the candidates this scan enumerates, not over every call a
compiler sees: a lower-confidence number than a compiler index. Precision is the headline metric.

Usage:
  lsp2scip.py --root DIR --ext .go[,.mod] --language-id go --out FILE.scip -- SERVER [ARG...]
"""

import argparse
import json
import os
import pathlib
import re
import socket
import subprocess
import sys
import threading
import time
import urllib.parse

CALL = re.compile(r"(?<![\w$])([A-Za-z_$][\w$]*)\s*\(")
SIMPLE = re.compile(r"[A-Za-z0-9_+\-$]+")
SKIP_DIRS = {".git", "node_modules", "vendor", "third_party", "target", "build", "_build", "deps", ".venv"}
WINDOW = 256
TOOL = "lsp2scip"
TOOL_VERSION = "1"
DEFINITION = 1


# --- protobuf wire encoding for the SCIP subset this bridge writes -------------------------------

def varint(value):
    out = bytearray()
    while True:
        byte = value & 0x7F
        value >>= 7
        if value:
            out.append(byte | 0x80)
        else:
            out.append(byte)
            return bytes(out)


def field_bytes(number, payload):
    return varint((number << 3) | 2) + varint(len(payload)) + payload


def field_str(number, text):
    return field_bytes(number, text.encode("utf-8"))


def field_int(number, value):
    return varint(number << 3) + varint(value)


def packed_ints(number, values):
    return field_bytes(number, b"".join(varint(v) for v in values))


def occurrence_bytes(rng, symbol, roles):
    out = packed_ints(1, rng) + field_str(2, symbol)
    if roles:
        out += field_int(3, roles)
    return out


def document_bytes(rel, language, occurrences):
    out = field_str(1, rel)
    for rng, symbol, roles in occurrences:
        out += field_bytes(2, occurrence_bytes(rng, symbol, roles))
    return out + field_str(4, language)


def index_bytes(project_root, arguments, documents):
    tool = field_str(1, TOOL) + field_str(2, TOOL_VERSION)
    for arg in arguments:
        tool += field_str(3, arg)
    metadata = field_int(1, 0) + field_bytes(2, tool) + field_str(3, project_root) + field_int(4, 1)
    out = field_bytes(1, metadata)
    for rel, language, occurrences in documents:
        out += field_bytes(2, document_bytes(rel, language, occurrences))
    return out


# --- SCIP symbol strings -------------------------------------------------------------------------

def escape(name):
    if SIMPLE.fullmatch(name):
        return name
    return "`" + name.replace("`", "``") + "`"


def symbol_for(namespace, name, line, char):
    """A global symbol string: the file as namespace, the callee name as a method whose
    disambiguator pins the definition position, so distinct definitions never share a string."""
    return f"lsp ripwire . . {escape(namespace)}/{escape(name)}({line}_{char})."


# --- LSP client ----------------------------------------------------------------------------------

CONNECT_WAIT = 120.0


def connect_tcp(address, wait):
    """Streams for a language server that listens on HOST:PORT; retried until WAIT seconds pass."""
    host, _, port = address.rpartition(":")
    deadline = time.monotonic() + wait
    while True:
        try:
            sock = socket.create_connection((host or "127.0.0.1", int(port)), timeout=5)
        except OSError:
            if time.monotonic() > deadline:
                raise
            time.sleep(1)
            continue
        sock.settimeout(None)
        return sock.makefile("rb"), sock.makefile("wb")


class Client:
    """One language server process. A reader thread answers server-initiated requests with null
    (or the configuration object for workspace/configuration) and files responses by id."""

    def __init__(self, command, cwd, config, connect=""):
        if connect:
            self.proc = None
            self.reader, self.writer = connect_tcp(connect, CONNECT_WAIT)
        else:
            self.proc = subprocess.Popen(command, cwd=cwd, stdin=subprocess.PIPE, stdout=subprocess.PIPE)
            self.reader, self.writer = self.proc.stdout, self.proc.stdin
        self.config = config
        self.lock = threading.Lock()
        self.cond = threading.Condition()
        self.responses = {}
        self.next_id = 0
        self.closed = False
        threading.Thread(target=self.read_loop, daemon=True).start()

    def exited(self):
        return self.proc is not None and self.proc.poll() is not None

    def read_message(self):
        length = None
        while True:
            line = self.reader.readline()
            if not line:
                return None
            if line in (b"\r\n", b"\n"):
                break
            key, _, value = line.decode("ascii", "replace").partition(":")
            if key.strip().lower() == "content-length":
                length = int(value)
        return json.loads(self.reader.read(length)) if length is not None else None

    def read_loop(self):
        while True:
            message = self.read_message()
            if message is None:
                break
            if "method" in message and "id" in message:
                self.answer_server_request(message)
            elif "id" in message:
                with self.cond:
                    self.responses[message["id"]] = message
                    self.cond.notify_all()
        with self.cond:
            self.closed = True
            self.cond.notify_all()

    def answer_server_request(self, message):
        result = None
        if message["method"] == "workspace/configuration":
            result = [self.config for _ in message.get("params", {}).get("items", [])]
        self.write({"jsonrpc": "2.0", "id": message["id"], "result": result})

    def write(self, message):
        body = json.dumps(message).encode("utf-8")
        with self.lock:
            self.writer.write(b"Content-Length: %d\r\n\r\n" % len(body) + body)
            self.writer.flush()

    def notify(self, method, params):
        self.write({"jsonrpc": "2.0", "method": method, "params": params})

    def send(self, method, params):
        with self.cond:
            self.next_id += 1
            ident = self.next_id
        self.write({"jsonrpc": "2.0", "id": ident, "method": method, "params": params})
        return ident

    def wait(self, idents, timeout):
        """Block until every ident is answered. TIMEOUT is idle time: seconds with no answer at all."""
        with self.cond:
            last = time.monotonic()
            seen = len(self.responses)
            while not all(i in self.responses for i in idents):
                if self.closed or self.exited():
                    raise RuntimeError("language server exited before answering")
                if len(self.responses) != seen:
                    seen, last = len(self.responses), time.monotonic()
                if time.monotonic() - last > timeout:
                    raise RuntimeError(f"language server silent for {timeout}s")
                self.cond.wait(1.0)
            return {i: self.responses.pop(i) for i in idents}

    def call(self, method, params, timeout):
        ident = self.send(method, params)
        message = self.wait([ident], timeout)[ident]
        if "error" in message:
            raise RuntimeError(f"{method} failed: {message['error']}")
        return message.get("result")

    def shutdown(self):
        try:
            self.call("shutdown", None, 60)
            self.notify("exit", None)
        except (RuntimeError, OSError):
            pass
        if self.proc is not None:
            try:
                self.proc.wait(timeout=30)
            except subprocess.TimeoutExpired:
                self.proc.kill()
                self.proc.wait()
        self.writer.close()
        self.reader.close()


# --- enumeration and answers ---------------------------------------------------------------------

def source_files(root, exts):
    found = []
    for dirpath, dirnames, filenames in os.walk(root):
        dirnames[:] = sorted(d for d in dirnames if d not in SKIP_DIRS and not d.startswith("."))
        for name in sorted(filenames):
            if os.path.splitext(name)[1] in exts:
                found.append(os.path.relpath(os.path.join(dirpath, name), root))
    return sorted(found)


def to_path(uri):
    parsed = urllib.parse.urlsplit(uri)
    return os.path.realpath(urllib.parse.unquote(parsed.path))


def utf16_col(text, col):
    return len(text[:col].encode("utf-16-le")) // 2


def candidates(rel, text, pattern=CALL):
    """(file, line, column in code points, callee name, column in UTF-16 units) per PATTERN match:
    group 1 is the callee name."""
    found = []
    for line_no, line in enumerate(text.split("\n")):
        for match in pattern.finditer(line):
            col = match.start(1)
            found.append((rel, line_no, col, match.group(1), utf16_col(line, col)))
    return found


def locations(answer):
    """Distinct (path, line, start char, end char) targets of one definition answer."""
    if answer is None:
        return []
    items = [answer] if isinstance(answer, dict) else answer
    out = set()
    for item in items:
        if "targetUri" in item:
            uri, rng = item["targetUri"], item.get("targetSelectionRange", item.get("targetRange"))
        else:
            uri, rng = item.get("uri"), item.get("range")
        if uri and rng:
            out.add((to_path(uri), rng["start"]["line"], rng["start"]["character"], rng["end"]["character"]))
    return sorted(out)


def decide(root, answer):
    """(kind, where, line, char, end): one in-repo definition (relative path) or one external one
    (absolute path); None when the answer names no single definition."""
    found = locations(answer)
    if len(found) != 1:
        return None
    path, line, char, end = found[0]
    rel = os.path.relpath(path, root)
    if rel.startswith(".."):
        return ("external", path, line, char, end)
    return ("in-repo", rel, line, char, end)


def ask_definitions(client, batch, root, timeout):
    """(candidate, decision) pairs; requests go out in windows of WINDOW so the server stays busy."""
    decided = []
    for start in range(0, len(batch), WINDOW):
        chunk = batch[start:start + WINDOW]
        idents = [
            client.send("textDocument/definition", {
                "textDocument": {"uri": pathlib.Path(root, rel).as_uri()},
                "position": {"line": line, "character": col16},
            })
            for rel, line, _, _, col16 in chunk
        ]
        answers = client.wait(idents, timeout)
        for cand, ident in zip(chunk, idents):
            message = answers[ident]
            result = None if "error" in message else message.get("result")
            decided.append((cand, decide(root, result)))
    return decided


def open_documents(client, root, rels, language):
    for rel in rels:
        path = pathlib.Path(root, rel)
        client.notify("textDocument/didOpen", {"textDocument": {
            "uri": path.as_uri(), "languageId": language, "version": 1,
            "text": path.read_text(encoding="utf-8", errors="replace"),
        }})


def build_documents(rels, decided, language):
    """SCIP documents: one per scanned file, plus one per in-repo file that holds a definition
    (ripwire pins references only to definitions in documents it can see)."""
    occurrences = {rel: [] for rel in rels}
    seen_defs = set()
    for (rel, line, col, name, _), target in decided:
        if target is None:
            continue
        kind, where, tline, tchar, tend = target
        symbol = symbol_for(where, name, tline, tchar)
        if kind == "in-repo" and (where, tline, tchar) not in seen_defs:
            seen_defs.add((where, tline, tchar))
            occurrences.setdefault(where, []).append(([tline, tchar, tend], symbol, DEFINITION))
        occurrences[rel].append(([line, col, col + len(name)], symbol, 0))
    return [(rel, language, occ) for rel, occ in sorted(occurrences.items())]


def scan_candidates(root, rels, pattern=CALL):
    batch = []
    for rel in rels:
        text = pathlib.Path(root, rel).read_text(encoding="utf-8", errors="replace")
        batch.extend(candidates(rel, text, pattern))
    return batch


def start_client(args, root):
    build = Client
    return build(args.server, root, json.loads(args.config), args.connect)


def index_repo(args):
    root = os.path.realpath(args.root)
    exts = {e if e.startswith(".") else "." + e for e in args.ext.split(",") if e}
    rels = source_files(root, exts)
    if not rels:
        raise SystemExit(f"lsp2scip: no {args.ext} files under {root}")
    client = start_client(args, root)
    try:
        client.call("initialize", {
            "processId": os.getpid(),
            "rootUri": pathlib.Path(root).as_uri(),
            "workspaceFolders": [{"uri": pathlib.Path(root).as_uri(), "name": "repo"}],
            "capabilities": {"workspace": {"configuration": True, "workspaceFolders": True},
                             "textDocument": {"definition": {"linkSupport": True}}},
            "initializationOptions": json.loads(args.init_options),
        }, args.timeout)
        client.notify("initialized", {})
        open_documents(client, root, rels, args.language_id)
        time.sleep(args.settle)
        batch = scan_candidates(root, rels, re.compile(args.pattern))
        print(f"lsp2scip: {len(rels)} files, {len(batch)} candidates", file=sys.stderr)
        decided = ask_definitions(client, batch, root, args.timeout)
    finally:
        client.shutdown()
    answered = sum(1 for _, target in decided if target is not None)
    print(f"lsp2scip: {answered} candidates with one definition, {len(decided) - answered} without",
          file=sys.stderr)
    documents = build_documents(rels, decided, args.language_id)
    pathlib.Path(args.out).write_bytes(index_bytes(pathlib.Path(root).as_uri(), args.server, documents))


def build_parser():
    parser = argparse.ArgumentParser(description="LSP definitions to a SCIP index.")
    parser.add_argument("--root", required=True)
    parser.add_argument("--ext", required=True, help="comma-separated source extensions")
    parser.add_argument("--language-id", required=True)
    parser.add_argument("--out", required=True)
    parser.add_argument("--init-options", default="{}", help="JSON initializationOptions")
    parser.add_argument("--config", default="{}", help="JSON object returned for workspace/configuration")
    parser.add_argument("--timeout", type=float, default=600.0, help="seconds without any server answer")
    parser.add_argument("--settle", type=float, default=0.0,
                        help="seconds to let the server analyse the opened documents before asking")
    parser.add_argument("--pattern", default=CALL.pattern,
                        help="regex whose group 1 is the callee name at each candidate call site")
    parser.add_argument("--connect", default="",
                        help="HOST:PORT of a server already listening (Godot); SERVER is then not started")
    parser.add_argument("server", nargs=argparse.REMAINDER, help="-- SERVER [ARG...]")
    return parser


def main(argv=None):
    args = build_parser().parse_args(argv)
    if args.server and args.server[0] == "--":
        args.server = args.server[1:]
    if not args.server and not args.connect:
        sys.exit("lsp2scip: give the server command after -- (or --connect HOST:PORT)")
    index_repo(args)
    return 0


if __name__ == "__main__":
    sys.exit(main())
