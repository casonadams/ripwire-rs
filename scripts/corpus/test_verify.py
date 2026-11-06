"""Unit checks for the pure parts of verify.py: judging, the ratchet's refusals, floor raising, changed-file scope
and table text. No Docker, no network, no binaries."""

import pathlib
import sys
import tomllib
import unittest
import unittest.mock

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

import corpus
import verify


def entry(name="demo", languages=("python",), precision_floor=0.98, recall_floor=0.9, status="measured"):
    return corpus.Entry.from_table({
        "name": name, "languages": list(languages), "indexer": "scip-python", "indexer_version": "1.0",
        "status": status, "precision_floor": precision_floor, "recall_floor": recall_floor,
        "reason": "not here" if status == "blocked" else "",
    })


def row(p=0.99, r=0.95, pin="abc", name="demo", **floors):
    return verify.Row(entry(name, **floors), pin=pin, calls=10, covered=8, precision=p, recall=r)


def record(p, r, pin="abc", name="demo"):
    return {"name": name, "precision": p, "recall": r, "sha": pin, "indexer_version": "1.0"}


class Judge(unittest.TestCase):
    def test_floor_miss_fails_with_the_metric_named(self):
        verdict = verify.judge(row(p=0.97), None)
        self.assertEqual(verdict.status, "FAIL")
        self.assertIn("precision 0.9700 below floor 0.9800", verdict.reasons[0])

    def test_drop_beyond_noise_fails(self):
        verdict = verify.judge(row(p=0.9900), record(0.9920, 0.95))
        self.assertEqual(verdict.status, "FAIL")
        self.assertIn("dropped from 0.9920", verdict.reasons[0])

    def test_drop_within_noise_margin_passes(self):
        verdict = verify.judge(row(p=0.98975), record(0.9900, 0.95))
        self.assertEqual(verdict.status, "ok")

    def test_entry_without_baseline_is_new_not_failed(self):
        self.assertEqual(verify.judge(row(), None).status, "NEW")

    def test_improvement_is_ok_and_raisable(self):
        verdict = verify.judge(row(p=0.9900, r=0.95), record(0.9800, 0.95))
        self.assertEqual(verdict.status, "ok")
        self.assertTrue(verify.raisable(verdict))

    def test_skipped_row_is_skip_with_its_reason(self):
        verdict = verify.judge(verify.Row(entry(), skip="no cached index demo.scip"), None)
        self.assertEqual((verdict.status, verdict.reasons), ("SKIP", ("no cached index demo.scip",)))

    def test_self_check_problem_fails_even_when_scip_skipped(self):
        skipped = verify.Row(entry(), skip="rust-analyzer unavailable", problem="make resolution fails (exit 1)")
        self.assertEqual(verify.judge(skipped, None).status, "FAIL")

    def test_unmeasured_metric_with_floor_fails(self):
        self.assertEqual(verify.judge(row(p=None), None).status, "FAIL")

    def test_record_for_another_pin_is_not_comparable(self):
        self.assertFalse(verify.comparable(record(0.99, 0.95, pin="old"), row(pin="new")))
        self.assertFalse(verify.comparable(None, row()))
        self.assertTrue(verify.comparable(record(0.99, 0.95), row()))


class Ratchet(unittest.TestCase):
    def test_refuses_any_drop_even_within_noise(self):
        verdicts = [verify.judge(row(p=0.9897), record(0.9900, 0.95))]
        refusals, _, records = verify.plan_ratchet(verdicts, {"demo": record(0.9900, 0.95)})
        self.assertEqual(len(refusals), 1)
        self.assertIn("below baseline 0.9900", refusals[0])
        self.assertEqual(records["demo"]["precision"], 0.9900)

    def test_refuses_a_failed_entry(self):
        refusals, _, _ = verify.plan_ratchet([verify.judge(row(p=0.5), None)], {})
        self.assertEqual(len(refusals), 1)

    def test_refuses_an_unmeasured_metric(self):
        unfloored = verify.Row(entry(precision_floor=None, recall_floor=None), pin="abc", precision=None, recall=0.95)
        refusals, _, _ = verify.plan_ratchet([verify.judge(unfloored, None)], {})
        self.assertIn("precision unmeasured", refusals[0])

    def test_raises_measured_entries_and_keeps_unmeasured_records(self):
        verdicts = [
            verify.judge(row(p=0.9900, r=0.95), record(0.9800, 0.94)),
            verify.judge(verify.Row(entry("other"), skip="no cached index"), None),
        ]
        baseline = {"other": record(0.9, 0.8, name="other")}
        refusals, _, records = verify.plan_ratchet(verdicts, baseline)
        self.assertEqual(refusals, [])
        self.assertEqual((records["demo"]["precision"], records["demo"]["recall"]), (0.99, 0.95))
        self.assertEqual(records["other"], baseline["other"])

    def test_repin_starts_the_record_again_with_a_note(self):
        verdicts = [verify.judge(row(p=0.9500, pin="new", precision_floor=0.5, recall_floor=0.5), None)]
        refusals, notes, records = verify.plan_ratchet(verdicts, {"demo": record(0.9900, 0.95, pin="old")})
        self.assertEqual(refusals, [])
        self.assertEqual(len(notes), 1)
        self.assertEqual(records["demo"]["precision"], 0.95)


class Floors(unittest.TestCase):
    def test_raises_to_measured_minus_headroom(self):
        verdicts = [verify.judge(row(p=0.9868, r=0.9789), record(0.9868, 0.9789))]
        raised = verify.plan_floors(verdicts, {"demo": record(0.9868, 0.9789)})
        self.assertEqual(raised, {"demo": {"precision_floor": 0.9848, "recall_floor": 0.9769}})

    def test_never_lowers_a_floor(self):
        verdicts = [verify.judge(row(p=0.9868, r=0.9789, precision_floor=0.9900, recall_floor=0.9900),
                                 record(0.9868, 0.9789))]
        self.assertEqual(verify.plan_floors(verdicts, {"demo": record(0.9868, 0.9789)}), {})

    def test_set_floors_edits_only_the_named_block(self):
        text = (
            '[[repo]]\nname = "alpha"\nprecision_floor = 0.9100\nrecall_floor = 0.9200\n\n'
            '[[repo]]\nname = "beta"\nprecision_floor = 0.9100\nrecall_floor = 0.9200\n'
        )
        out = verify.set_floors(text, {"beta": {"precision_floor": 0.95}})
        self.assertEqual(out.count("precision_floor = 0.9500"), 1)
        self.assertIn('name = "alpha"\nprecision_floor = 0.9100\n', out)
        self.assertIn("recall_floor = 0.9200", out)

    def test_render_round_trips_through_toml(self):
        records = {"demo": record(0.9868, 0.9789), "ripwire-rs-self": record(0.9, 0.8, name="ripwire-rs-self")}
        parsed = {rec["name"]: rec for rec in tomllib.loads(verify.render_baseline(records))["repo"]}
        self.assertEqual(parsed["demo"]["precision"], 0.9868)
        self.assertEqual(parsed["demo"]["sha"], "abc")
        self.assertEqual(parsed["ripwire-rs-self"]["recall"], 0.8)


class Scope(unittest.TestCase):
    KNOWN = {"c", "cpp", "typescript", "javascript", "rust", "go", "ruby"}

    def scope(self, *paths):
        return verify.affected_languages(list(paths), self.KNOWN)

    def test_language_directory_maps_to_that_language(self):
        self.assertEqual(self.scope("src/languages/ruby/tags.scm"), {"ruby"})

    def test_shared_core_touches_every_entry(self):
        self.assertIsNone(self.scope("src/resolve/graph.rs"))
        self.assertIsNone(self.scope("src/ingest/walk.rs"))
        self.assertIsNone(self.scope("src/languages/common/names.rs"))

    def test_language_test_file_maps_by_name_prefix(self):
        self.assertEqual(self.scope("tests/languages/typescript_bindings.rs"), {"typescript"})
        self.assertEqual(self.scope("tests/languages/c.rs"), {"c"})
        self.assertEqual(self.scope("tests/languages/cpp.rs"), {"cpp"})

    def test_shared_test_helper_touches_every_entry(self):
        self.assertIsNone(self.scope("tests/languages/harness.rs"))

    def test_unrelated_paths_touch_nothing(self):
        self.assertEqual(self.scope("docs/GATES.md", "scripts/corpus/verify.py"), set())
        self.assertEqual(self.scope(), set())

    def test_mixed_changes_union_and_a_whole_tree_file_wins(self):
        self.assertEqual(self.scope("docs/x.md", "src/languages/go/mod.rs"), {"go"})
        self.assertIsNone(self.scope("src/languages/go/mod.rs", "Cargo.lock"))


class Table(unittest.TestCase):
    def test_columns_and_delta_text(self):
        verdicts = [verify.judge(row(p=0.9900, r=0.9500), record(0.9800, 0.9500))]
        lines = verify.format_table(verdicts).splitlines()
        self.assertIn("precision", lines[0])
        self.assertIn("result", lines[0])
        self.assertIn("+0.0100", lines[1])
        self.assertTrue(lines[1].endswith("ok"))

    def test_skipped_row_states_its_reason(self):
        verdict = verify.judge(verify.Row(entry(), skip="no cached index demo.scip"), None)
        self.assertIn("SKIP: no cached index demo.scip", verify.format_table([verdict]))

    def test_pass_verdict_counts_and_fail_verdict_names_the_entry(self):
        ok = verify.judge(row(), None)
        skipped = verify.judge(verify.Row(entry("other"), skip="x"), None)
        self.assertEqual(verify.verdict_line([ok, skipped]),
                         "VERIFY PASS: 1 measured, 1 skipped, 1 new, 0 raisable")
        failed = verify.judge(row(p=0.5, name="rack"), None)
        self.assertEqual(verify.verdict_line([failed]).split(" (")[0], "VERIFY FAIL: rack")


class SelfCheck(unittest.TestCase):
    def test_scores_come_from_the_precision_and_recall_lines(self):
        text = "ok   graph_ambiguous=572 (bound 583)\nok   precision=0.9868 (bound 0.9855)\n" \
               "FAIL recall=0.9700 (bound 0.976)\n"
        self.assertEqual(verify.parse_self(text), (0.9868, 0.97, ""))

    def test_unscored_scip_is_a_skip_with_a_reason(self):
        precision, recall, skip = verify.parse_self("skip SCIP precision/recall: rust-analyzer unavailable\n")
        self.assertEqual((precision, recall), (None, None))
        self.assertTrue(skip)

    def test_none_value_is_unmeasured(self):
        self.assertEqual(verify.parse_self("ok   precision=None (bound 0.9)\n")[0], None)


class Flips(unittest.TestCase):
    OLD = '<verdict v="exact" p="a.rb:1" callee="f"/><verdict v="exact" p="a.rb:9" callee="g"/>'
    NEW = ('<verdict v="wrong" p="a.rb:1" callee="f"/><verdict v="exact" p="a.rb:9" callee="g"/>'
           '<site v="wrong" why="name" p="a.rb:1" callee="f" scip="f@x:2" ours="f@y:3"/>')

    def test_right_to_wrong_names_the_site_with_ours_and_scip(self):
        report = verify.diff_report("demo", self.OLD, {"precision": "1.0"}, self.NEW, {"precision": "0.5"})
        self.assertEqual(report.counts["right->wrong"], 1)
        self.assertIn("  right->wrong a.rb:1 callee=f ours=f@y:3 scip=f@x:2", report.lines)
        self.assertIn("demo: precision 1.0 -> 0.5", report.lines[0])

    def test_unsampled_site_shows_dashes(self):
        report = verify.diff_report("demo", self.OLD, {}, self.NEW.replace("<site", "<x"), {})
        self.assertIn("  right->wrong a.rb:1 callee=f ours=- scip=-", report.lines)

    def test_no_change_has_no_flips(self):
        report = verify.diff_report("demo", self.OLD, {}, self.OLD, {})
        self.assertEqual(sum(report.counts.values()), 0)


class Sites(unittest.TestCase):
    def test_site_details_key_by_location_and_callee(self):
        text = ('<site v="wrong" why="local" p="a.rb:1" callee="f" scip="f@x:2" ours="f@y:3"/>\n'
                '<site v= why= p= callee= scip= ours=>\n')
        found = verify.site_details(text)
        self.assertEqual(found[("a.rb:1", "f")]["scip"], "f@x:2")
        self.assertEqual(len(found), 1)


class Dispatch(unittest.TestCase):
    def test_corpus_verify_hands_the_rest_of_argv_to_verify(self):
        with unittest.mock.patch.object(verify, "main", return_value=7) as main:
            self.assertEqual(corpus.main(["verify", "status", "rack"]), 7)
        main.assert_called_once_with(["status", "rack"])


class Identity(unittest.TestCase):
    def test_tree_pin_and_sha_pin_give_their_index_keys(self):
        self.assertEqual(verify.index_key("tree:0123456789abcdef" + "0" * 48), "0123456789ab")
        self.assertEqual(verify.index_key("9d8f9a372cc5916fd4781fec207ced7ddac2f08f"), "9d8f9a372cc5")

    def test_blocked_entry_is_skipped_without_touching_disk(self):
        blocked = verify.measure(entry("plug", status="blocked"), "/nonexistent/bin")
        self.assertIn("blocked", blocked.skip)


if __name__ == "__main__":
    unittest.main()
