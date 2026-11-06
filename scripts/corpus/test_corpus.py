"""Unit checks for the pure helpers in corpus.py (no Docker, no network)."""

import importlib.util
import pathlib
import unittest

_SPEC = importlib.util.spec_from_file_location("corpus", pathlib.Path(__file__).with_name("corpus.py"))
corpus = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(corpus)

VERDICTS = (
    '<scip-verdicts>'
    '<verdict v="exact" p="src/a.rs:10" callee="build"/>'
    '<verdict v="wrong" p="src/b.rs:3" callee="open"/>'
    '<verdict v="unindexed" p="src/c.rs:7" callee="far"/>'
    '<verdict v="false-edge" p="src/a.rs:10" callee="build"/>'
    '</scip-verdicts>'
)


def table(**fields):
    base = {"name": "demo", "indexer": "scip-python"}
    base.update(fields)
    return base


class ParseStates(unittest.TestCase):
    def test_states_by_site_and_worst_reference_wins(self):
        states = corpus.parse_states(VERDICTS)
        self.assertEqual(states[("src/a.rs:10", "build")], "wrong")
        self.assertEqual(states[("src/b.rs:3", "open")], "wrong")
        self.assertEqual(states[("src/c.rs:7", "far")], "neutral")
        self.assertEqual(len(states), 3)

    def test_ignores_non_verdict_text(self):
        self.assertEqual(corpus.parse_states('<scip-eval precision="1"/>'), {})

    def test_unknown_verdict_is_loud(self):
        with self.assertRaises(KeyError):
            corpus.parse_states('<verdict v="bogus" p="a:1" callee="f"/>')


class Flips(unittest.TestCase):
    def test_each_flip_is_classified_exactly(self):
        old = {("a", "x"): "right", ("b", "x"): "wrong", ("c", "x"): "right",
               ("d", "x"): "neutral", ("e", "x"): "wrong", ("f", "x"): "wrong",
               ("h", "x"): "wrong"}
        new = {("a", "x"): "wrong", ("b", "x"): "right", ("c", "x"): "neutral",
               ("d", "x"): "wrong", ("e", "x"): "wrong", ("g", "x"): "wrong"}
        self.assertEqual(corpus.flips(old, new), {
            "right->wrong": [("a", "x")],
            "wrong->right": [("b", "x")],
            "entered": [("d", "x"), ("g", "x")],
            "left": [("f", "x"), ("h", "x")],
        })

    def test_right_to_neutral_and_wrong_to_wrong_are_not_flips(self):
        flipped = corpus.flips({("a", "x"): "right"}, {("a", "x"): "neutral"})
        self.assertTrue(all(keys == [] for keys in flipped.values()), flipped)
        still = corpus.flips({("a", "x"): "wrong"}, {("a", "x"): "wrong"})
        self.assertEqual(still["entered"], [])
        self.assertEqual(still["left"], [])


class EvalCommand(unittest.TestCase):
    def test_verdicts_run_with_the_sites_flag(self):
        calls = []

        class Done:
            returncode = 0
            stdout = '<scip-eval precision="1"/>'
            stderr = ""

        original = corpus.subprocess.run
        corpus.subprocess.run = lambda args, **kwargs: calls.append(args) or Done()
        try:
            corpus.scip_verdicts("bin", "src", "idx.scip")
            corpus.scip_eval("bin", "src", "idx.scip", 0)
        finally:
            corpus.subprocess.run = original
        self.assertIn("--eval-sites", calls[0])
        self.assertNotIn("--eval-sites", calls[1])


class Floors(unittest.TestCase):
    def test_suggestion_keeps_margin_and_rounds_down(self):
        self.assertEqual(corpus.suggested(0.9858), 0.9838)
        self.assertEqual(corpus.suggested(0.97749), 0.9754)

    def test_missing_measurement_is_a_miss(self):
        entry = corpus.Entry.from_table(table(precision_floor=0.9, recall_floor=0.9))
        self.assertEqual(corpus.Score(entry, 0, 0, None, 0.95).misses(), ["precision"])

    def test_no_floor_never_misses(self):
        entry = corpus.Entry.from_table(table())
        self.assertEqual(corpus.Score(entry, 1, 1, 0.1, 0.1).misses(), [])

    def test_below_floor_misses_only_that_metric(self):
        entry = corpus.Entry.from_table(table(precision_floor=0.9, recall_floor=0.9))
        self.assertEqual(corpus.Score(entry, 1, 1, 0.95, 0.85).misses(), ["recall"])


class Manifest(unittest.TestCase):
    def test_defaults_for_unverified_entry(self):
        entry = corpus.Entry.from_table(table())
        self.assertEqual(entry.status, "unverified")
        self.assertEqual(entry.index_file, "index.scip")
        self.assertIsNone(entry.precision_floor)

    def test_container_script_substitutes_output_path(self):
        entry = corpus.Entry.from_table(table(setup="npm ci", index="scip-python index --output={out}"))
        self.assertEqual(corpus.container_script(entry), "npm ci\nscip-python index --output=/out/index.scip")

    def test_entry_selects_its_image_and_is_memory_capped(self):
        lsp = corpus.Entry.from_table(table(image="ripwire-corpus-lsp:local", index="lsp2scip --out {out}"))
        command = corpus.docker_command(lsp, pathlib.Path("/src"), pathlib.Path("/out"))
        self.assertIn("ripwire-corpus-lsp:local", command)
        self.assertNotIn(corpus.IMAGE, command)
        self.assertIn("--memory=3g", command)

    def test_default_image_is_the_scip_indexer_image(self):
        self.assertEqual(corpus.Entry.from_table(table()).image, corpus.IMAGE)


def field(number, payload):
    """One length-delimited protobuf field: the wire form SCIP uses for its documents."""
    key = (number << 3) | 2
    length, size = bytearray(), len(payload)
    while True:
        length.append((size & 0x7F) | (0x80 if size > 0x7F else 0))
        size >>= 7
        if not size:
            break
    return bytes([key]) + bytes(length) + payload


def document(path):
    return field(1, path.encode())


class SubdirIndex(unittest.TestCase):
    def test_keeps_only_documents_under_the_subdirectory(self):
        index = field(1, b"meta") + field(2, document("src/a.php")) + field(2, document("tests/t.php")) + field(3, b"ext")
        self.assertEqual(corpus.scip_subdir(index, "src"), field(1, b"meta") + field(2, document("src/a.php")) + field(3, b"ext"))

    def test_prefix_matches_a_whole_path_component(self):
        index = field(2, document("srcx/b.php")) + field(2, document("src/c.php"))
        self.assertEqual(corpus.scip_subdir(index, "src/"), field(2, document("src/c.php")))

    def test_long_paths_with_multi_byte_lengths_survive(self):
        long_path = "src/" + "a" * 300 + ".php"
        index = field(2, document(long_path)) + field(2, document("other/x.php"))
        self.assertEqual(corpus.scip_subdir(index, "src"), field(2, document(long_path)))

    def test_unsupported_wire_type_is_refused(self):
        with self.assertRaisesRegex(ValueError, "wire type 3"):
            list(corpus.wire_fields(bytes([0x0B])))

    def test_subdir_entry_mounts_the_whole_checkout(self):
        entry = corpus.Entry.from_table(table(subdir="src"))
        command = corpus.docker_command(entry, pathlib.Path("/checkout"), pathlib.Path("/out"))
        self.assertIn("/checkout:/src", command)


if __name__ == "__main__":
    unittest.main()
