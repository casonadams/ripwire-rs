"""Unit checks for the pure helpers in corpus.py (no Docker, no network)."""

import importlib.util
import pathlib
import unittest

_SPEC = importlib.util.spec_from_file_location("corpus", pathlib.Path(__file__).with_name("corpus.py"))
corpus = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(corpus)

SITES = (
    '<site v="wrong" why="declined" p="src/a.rs:10" callee="build" scip="x@1" ours="y@2"/>'
    '<site v="missed" why="declined" p="src/b.rs:3" callee="open" scip="z@3" ours=""/>'
)


def table(**fields):
    base = {"name": "demo", "indexer": "scip-python"}
    base.update(fields)
    return base


class ParseSites(unittest.TestCase):
    def test_keys_by_site_and_callee(self):
        sites = corpus.parse_sites(SITES)
        self.assertEqual(sites[("src/a.rs:10", "build")]["v"], "wrong")
        self.assertEqual(sites[("src/b.rs:3", "open")]["v"], "missed")

    def test_ignores_non_site_text(self):
        self.assertEqual(corpus.parse_sites('<scip-eval precision="1"/>'), {})


class Classify(unittest.TestCase):
    def test_entered_left_and_changed(self):
        old = corpus.parse_sites(SITES)
        new = corpus.parse_sites(
            '<site v="wrong" why="declined" p="src/c.rs:1" callee="new" scip="" ours="q@1"/>'
            '<site v="false-edge" why="declined" p="src/a.rs:10" callee="build" scip="x@1" ours="y@2"/>'
        )
        entered, left, changed = corpus.classify(old, new)
        self.assertEqual(entered, [("src/c.rs:1", "new")])
        self.assertEqual(left, [("src/b.rs:3", "open")])
        self.assertEqual(changed, [("src/a.rs:10", "build")])


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


if __name__ == "__main__":
    unittest.main()
