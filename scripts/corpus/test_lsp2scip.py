"""Unit checks for the LSP bridge (lsp2scip.py): wire encoding, symbol strings, answer handling, and a
full run against a fake language server that answers go-to-definition from a `def NAME(` scan."""

import argparse
import importlib.util
import pathlib
import re
import sys
import tempfile
import textwrap
import unittest

_SPEC = importlib.util.spec_from_file_location("lsp2scip", pathlib.Path(__file__).with_name("lsp2scip.py"))
bridge = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(bridge)

FAKE_SERVER = textwrap.dedent('''
    import json, re, sys, pathlib
    root = None
    def send(message):
        body = json.dumps(message).encode()
        sys.stdout.buffer.write(b"Content-Length: %d\\r\\n\\r\\n" % len(body) + body)
        sys.stdout.buffer.flush()
    def read():
        length = None
        while True:
            line = sys.stdin.buffer.readline()
            if not line:
                return None
            if line in (b"\\r\\n", b"\\n"):
                break
            if line.lower().startswith(b"content-length:"):
                length = int(line.split(b":")[1])
        return json.loads(sys.stdin.buffer.read(length))
    def definition(params):
        line, col = params["position"]["line"], params["position"]["character"]
        path = pathlib.Path(params["textDocument"]["uri"][len("file://"):])
        name = re.match(r"[A-Za-z_]\\w*", path.read_text().split("\\n")[line][col:]).group(0)
        for candidate in sorted(root.rglob("*.py")):
            for number, src in enumerate(candidate.read_text().split("\\n")):
                if re.match(r"\\s*def %s\\(" % name, src):
                    return {"uri": candidate.as_uri(), "range": {
                        "start": {"line": number, "character": 0}, "end": {"line": number, "character": 0}}}
        return None
    while True:
        message = read()
        if message is None:
            break
        method, ident = message.get("method"), message.get("id")
        if method == "initialize":
            root = pathlib.Path(message["params"]["rootUri"][len("file://"):])
            send({"jsonrpc": "2.0", "id": ident, "result": {"capabilities": {}}})
        elif method == "textDocument/definition":
            send({"jsonrpc": "2.0", "id": ident, "result": definition(message["params"])})
        elif method == "shutdown":
            send({"jsonrpc": "2.0", "id": ident, "result": None})
        elif method == "exit":
            break
''')


def read_varint(data, pos):
    value = shift = 0
    while True:
        byte = data[pos]
        pos += 1
        value |= (byte & 0x7F) << shift
        shift += 7
        if not byte & 0x80:
            return value, pos


def read_fields(data):
    """Decode one protobuf message into (field number, wire type, value) triples."""
    out, pos = [], 0
    while pos < len(data):
        key, pos = read_varint(data, pos)
        number, wire = key >> 3, key & 7
        if wire == 0:
            value, pos = read_varint(data, pos)
        else:
            length, pos = read_varint(data, pos)
            value, pos = data[pos:pos + length], pos + length
        out.append((number, wire, value))
    return out


def decode_documents(raw):
    """relative path -> [(symbol, roles)] for every document of a SCIP index."""
    docs = {}
    for number, _, doc in read_fields(raw):
        if number != 2:
            continue
        fields = read_fields(doc)
        rel = next(v for n, _, v in fields if n == 1).decode()
        docs[rel] = [decode_occurrence(v) for n, _, v in fields if n == 2]
    return docs


def decode_occurrence(raw):
    symbol, roles = "", 0
    for number, _, value in read_fields(raw):
        if number == 2:
            symbol = value.decode()
        elif number == 3:
            roles = value
    return symbol, roles


class Wire(unittest.TestCase):
    def test_varint_matches_protobuf_spec(self):
        self.assertEqual(bridge.varint(1), b"\x01")
        self.assertEqual(bridge.varint(300), b"\xac\x02")

    def test_occurrence_fields(self):
        fields = read_fields(bridge.occurrence_bytes([3, 4, 9], "lsp ripwire . . a/b()", bridge.DEFINITION))
        self.assertEqual(fields[0][2], bridge.varint(3) + bridge.varint(4) + bridge.varint(9))
        self.assertEqual(fields[1][2], b"lsp ripwire . . a/b()")
        self.assertEqual(fields[2], (3, 0, 1))

    def test_document_lists_path_and_language(self):
        numbers = [n for n, _, _ in read_fields(bridge.document_bytes("pkg/a.go", "go", [([1, 2, 5], "s", 0)]))]
        self.assertEqual(numbers, [1, 2, 4])


class Symbols(unittest.TestCase):
    def test_simple_names_stay_bare(self):
        self.assertEqual(bridge.escape("Run_2"), "Run_2")

    def test_other_names_are_backticked_and_escaped(self):
        self.assertEqual(bridge.escape("pkg/a.go"), "`pkg/a.go`")
        self.assertEqual(bridge.escape("x`y"), "`x``y`")

    def test_definition_symbol_pins_position(self):
        self.assertEqual(bridge.symbol_for("pkg/a.go", "Run", 3, 5), "lsp ripwire . . `pkg/a.go`/Run(3_5).")


class Answers(unittest.TestCase):
    def test_single_location_is_pinned(self):
        answer = {"uri": "file:///r/a.py", "range": {"start": {"line": 2, "character": 4},
                                                       "end": {"line": 2, "character": 7}}}
        self.assertEqual(bridge.decide("/r", answer), ("in-repo", "a.py", 2, 4, 7))

    def test_several_distinct_locations_are_ambiguous(self):
        span = {"start": {"line": 1, "character": 0}, "end": {"line": 1, "character": 1}}
        self.assertIsNone(bridge.decide("/r", [{"uri": "file:///r/a.py", "range": span},
                                               {"uri": "file:///r/b.py", "range": span}]))

    def test_location_link_uses_selection_range(self):
        link = {"targetUri": "file:///other/lib.py",
                "targetSelectionRange": {"start": {"line": 9, "character": 4}, "end": {"line": 9, "character": 6}},
                "targetRange": {"start": {"line": 8, "character": 0}, "end": {"line": 12, "character": 0}}}
        self.assertEqual(bridge.decide("/r", [link]), ("external", "/other/lib.py", 9, 4, 6))

    def test_no_answer_is_undecided(self):
        self.assertIsNone(bridge.decide("/r", None))

    def test_candidates_are_names_followed_by_paren(self):
        names = [c[3] for c in bridge.candidates("a.py", "x = foo(1)\ny.bar (2)\nz.qux\n")]
        self.assertEqual(names, ["foo", "bar"])

    def test_custom_pattern_names_the_first_matched_group(self):
        shell = re.compile(r"(?:^|[;&|]\s*)([A-Za-z_][\w-]*)(?=\s|$)")
        names = [c[3] for c in bridge.candidates("a.sh", "setup x; teardown\nrun_it | grep y\n", shell)]
        self.assertEqual(names, ["setup", "teardown", "run_it", "grep"])
        message = re.compile(r"\[\s*[^\[\]\s]+\s+([A-Za-z_]\w*)|\]\s+([A-Za-z_]\w*)")
        names = [c[3] for c in bridge.candidates("a.m", "[self.view addSubview:x];\n[[A alloc] init];\n", message)]
        self.assertEqual(names, ["addSubview", "alloc", "init"])

    def test_configuration_section_reads_nested_value(self):
        config = {"cobol-lsp": {"dialects": [], "cpy-manager": {"paths-local": ["cp"]}}}
        self.assertEqual(bridge.section_value(config, "cobol-lsp.dialects", config), [])
        self.assertEqual(bridge.section_value(config, "cobol-lsp.cpy-manager.paths-local", config), ["cp"])
        self.assertEqual(bridge.section_value(config, "cobol-lsp", config), config["cobol-lsp"])

    def test_absent_section_gets_the_default(self):
        config = {"gopls": {"a": 1}}
        self.assertEqual(bridge.section_value(config, "other.key", config), config)
        self.assertEqual(bridge.section_value({}, None, {}), {})


class EndToEnd(unittest.TestCase):
    def test_fake_server_produces_pinned_index(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = pathlib.Path(tmp, "repo")
            root.mkdir()
            (root / "lib.py").write_text("def helper(x):\n    return x\n")
            (root / "main.py").write_text("from lib import helper\n\ndef main():\n    return helper(1)\n")
            server = pathlib.Path(tmp, "server.py")
            server.write_text(FAKE_SERVER)
            out = pathlib.Path(tmp, "out.scip")
            args = argparse.Namespace(
                root=str(root), ext=".py", language_id="python", out=str(out), init_options="{}",
                config="{}", timeout=60.0, settle=0.0, pattern=bridge.CALL.pattern, connect="",
                server=[sys.executable, str(server)])
            bridge.index_repo(args)
            docs = decode_documents(out.read_bytes())
        self.assertEqual(set(docs), {"lib.py", "main.py"})
        helper = "lsp ripwire . . `lib.py`/helper(0_0)."
        self.assertIn((helper, bridge.DEFINITION), docs["lib.py"])
        self.assertIn((helper, 0), docs["main.py"])
        self.assertIn(("lsp ripwire . . `main.py`/main(2_0).", bridge.DEFINITION), docs["main.py"])


if __name__ == "__main__":
    unittest.main()
