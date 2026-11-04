#!/usr/bin/env python3
"""Unit checks for scripts/validate-parity.py root-attribute rules. Run: python3 scripts/test_validate_parity.py"""

import importlib.util
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "validate_parity", Path(__file__).resolve().parent / "validate-parity.py"
)
vp = importlib.util.module_from_spec(spec)
spec.loader.exec_module(vp)

UPSTREAM = '<panel schema="ripwire.panel/v1" families="4" window="12mo"/>'
OURS_EXTRA = '<panel schema="ripwire-rs.panel/v1" families="4" window="12mo" enabled="a,b"/>'
OURS_MISSING = '<panel schema="ripwire-rs.panel/v1" families="4"/>'


class RootAttributeRule(unittest.TestCase):
    def test_richer_entries_may_carry_ours_only_root_attributes(self):
        self.assertIsNone(vp.check_xml_parity("--quality-panel", OURS_EXTRA, UPSTREAM, allow_extra_root=True))

    def test_richer_entries_still_reject_a_missing_upstream_attribute(self):
        reason = vp.check_xml_parity("--quality-panel", OURS_MISSING, UPSTREAM, allow_extra_root=True)
        self.assertIn("missing upstream attrs", reason)

    def test_other_entries_reject_extra_root_attributes(self):
        reason = vp.check_xml_parity("--quality-panel", OURS_EXTRA, UPSTREAM)
        self.assertIn("extra=['enabled']", reason)


DOCTOR_UPSTREAM = '<doctor schema="ripwire.doctor/v1"><c n="cache-dir" ok="1" dir="/t" locks="0"/><note text="x"/></doctor>'
DOCTOR_OURS = '<doctor schema="ripwire-rs.doctor/v1"><c n="cache-dir" ok="1" dir="/t"/></doctor>'
DOCTOR_OURS_WITH_LOCKS = '<doctor schema="ripwire-rs.doctor/v1"><c n="cache-dir" ok="1" dir="/t" locks="0"/><note text="x"/></doctor>'


class OmissionRule(unittest.TestCase):
    def test_declared_row_attribute_omission_is_accepted(self):
        reason = vp.check_xml_parity("--doctor", DOCTOR_OURS, DOCTOR_UPSTREAM, omits=["cache-dir:locks", "note"])
        self.assertIsNone(reason)

    def test_undeclared_missing_row_attribute_still_fails(self):
        reason = vp.check_xml_parity("--doctor", DOCTOR_OURS, DOCTOR_UPSTREAM, omits=["note"])
        self.assertIn("has no row with required attrs", reason)

    def test_omission_is_stale_when_ours_emits_the_attribute(self):
        reason = vp.check_xml_parity("--doctor", DOCTOR_OURS_WITH_LOCKS, DOCTOR_UPSTREAM, omits=["cache-dir:locks", "note"])
        self.assertIn("omits not needed by this output", reason)

    def test_omission_for_another_row_does_not_exempt_this_row(self):
        reason = vp.check_xml_parity("--doctor", DOCTOR_OURS, DOCTOR_UPSTREAM, omits=["index-cache:locks", "note"])
        self.assertIn("has no row with required attrs ['dir', 'locks', 'n', 'ok']", reason)

    def test_undeclared_absent_element_fails_once_omits_are_declared(self):
        reason = vp.check_xml_parity("--doctor", DOCTOR_OURS, DOCTOR_UPSTREAM, omits=["cache-dir:locks"])
        self.assertIn("absent from ours and not declared", reason)

    def test_element_omission_is_accepted_only_when_it_is_really_absent(self):
        self.assertIsNone(vp.check_xml_parity("--doctor", DOCTOR_OURS, DOCTOR_UPSTREAM, omits=["cache-dir:locks", "note"]))
        reason = vp.check_xml_parity("--doctor", DOCTOR_OURS_WITH_LOCKS, DOCTOR_UPSTREAM, omits=["cache-dir:locks", "note"])
        self.assertIn("omits not needed", reason)

    def test_entries_without_omits_keep_the_element_rule_unchanged(self):
        self.assertIsNone(vp.check_xml_parity("--doctor", DOCTOR_OURS_WITH_LOCKS, DOCTOR_UPSTREAM))

    def test_omits_are_only_valid_on_smaller_entries_and_need_a_reason(self):
        entry = {"id": "x", "kind": "richer", "why": "locks and note", "omits": ["cache-dir:locks"]}
        self.assertTrue(any("only smaller entries" in e for e in vp.omit_declaration_errors(entry)))
        entry = {"id": "x", "kind": "smaller", "why": "note only", "omits": ["cache-dir:locks"]}
        self.assertTrue(any("has no reason naming 'locks'" in e for e in vp.omit_declaration_errors(entry)))
        entry = {"id": "x", "kind": "smaller", "why": "locks has no mechanism", "omits": ["cache-dir:locks"]}
        self.assertEqual(vp.omit_declaration_errors(entry), [])


if __name__ == "__main__":
    unittest.main()
