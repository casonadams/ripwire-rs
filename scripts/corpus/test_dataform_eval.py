"""Unit checks for the Dataform edge-set scorer (dataform_eval.py): which reads the compiled project implies."""

import importlib.util
import pathlib
import unittest

_SPEC = importlib.util.spec_from_file_location("dataform_eval", pathlib.Path(__file__).with_name("dataform_eval.py"))
scorer = importlib.util.module_from_spec(_SPEC)
_SPEC.loader.exec_module(scorer)


def target(name, schema="gold", database="proj"):
    return {"database": database, "schema": schema, "name": name}


def action(name, query="", deps=(), file=None):
    return {"target": target(name), "fileName": file or f"definitions/{name}.sqlx", "query": query,
            "dependencyTargets": list(deps)}


class TruthEdges(unittest.TestCase):
    def edges(self, **kinds):
        return scorer.truth_edges(kinds)

    def test_dependency_targets_are_reads(self):
        edges = self.edges(tables=[action("kind"), action("consumer", deps=[target("kind")])])
        self.assertEqual(edges, {("consumer", "kind")})

    def test_qualified_name_in_query_is_a_read_even_without_a_dependency(self):
        query = "SELECT 1 FROM `proj.gold.kind` k"
        edges = self.edges(tables=[action("kind")], assertions=[action("check", query=query)])
        self.assertEqual(edges, {("check", "kind")})

    def test_commented_mentions_and_other_schemas_are_not_reads(self):
        query = "-- FROM gold.kind\n/* proj.gold.kind */ SELECT 1 FROM silver.kind, other.gold.kindly"
        edges = self.edges(tables=[action("kind"), action("consumer", query=query)])
        self.assertEqual(edges, set())

    def test_js_actions_and_self_reads_are_ignored(self):
        edges = self.edges(tables=[
            action("kind"),
            action("built_in_js", deps=[target("kind")], file="definitions/built.js"),
            action("kind", query="SELECT * FROM proj.gold.kind"),
        ])
        self.assertEqual(edges, set())


class Score(unittest.TestCase):
    def test_precision_recall_and_differences(self):
        result = scorer.score({("a", "x"), ("b", "x"), ("c", "y")}, {("a", "x"), ("b", "y")})
        self.assertEqual((result["precision"], result["recall"]), (0.5, 1 / 3))
        self.assertEqual(result["missed"], [("b", "x"), ("c", "y")])
        self.assertEqual(result["false_edges"], [("b", "y")])


if __name__ == "__main__":
    unittest.main()
