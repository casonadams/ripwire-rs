"""Unit checks for corpus.py add and remove: license and SHA rules, file counting, rendering, floors, the
manifest and matrix edits, and the command flow. No network: a fake GitHub API and a fake git runner."""

import contextlib
import io
import pathlib
import sys
import tempfile
import tomllib
import unittest
from unittest import mock

HERE = pathlib.Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import add
import corpus

OWNER, REPO = "octo", "widgets"
URL = f"https://github.com/{OWNER}/{REPO}"
SHA = "0123456789abcdef0123456789abcdef01234567"
ROUTES = {
    f"/repos/{OWNER}/{REPO}": {
        "license": {"spdx_id": "MIT"}, "default_branch": "main", "archived": False, "fork": False,
    },
    f"/repos/{OWNER}/{REPO}/commits/main": {"sha": SHA},
    f"/repos/{OWNER}/{REPO}/commits/{SHA}": {"sha": SHA},
}
MANIFEST_TEXT = """# a header comment that must survive
# and a second one

[[repo]]
name = "alpha"
languages = ["python"]
status = "measured"
license = "MIT"
url = "https://github.com/a/alpha"
sha = "1111111111111111111111111111111111111111"
indexer = "scip-python"
indexer_version = "0.6.8"
index = "true"
precision_floor = 0.9800
recall_floor = 0.9000

[[repo]]
name = "beta"
languages = ["go"]
status = "measured"
license = "MIT"
url = "https://github.com/b/beta"
sha = "2222222222222222222222222222222222222222"
indexer = "lsp2scip"
indexer_version = "gopls-0.24.0"
index = "true"
precision_floor = 0.9500
recall_floor = 0.9500
"""
GATES_TEXT = """## 13. Resolver Corpus

| Language | Oracle | Status | Precision | Recall |
|---|---|---|---|---|
| go | LSP bridge (gopls) | measured | 0.9534 | 0.9586 |
| python | SCIP indexer (scip-python) | measured | 0.9860 | 0.9160 |
| rust | SCIP indexer (rust-analyzer) | measured | 0.9868 | 0.9789 |

Not covered.
"""


def fake_api(routes=ROUTES):
    def api(path):
        if path not in routes:
            raise add.AddError(f"no route {path}")
        return routes[path]
    return api


class FakeGit:
    """Records each git call and answers ls-files with LISTING."""

    def __init__(self, listing=""):
        self.listing = listing
        self.calls = []

    def __call__(self, args, cwd):
        self.calls.append((tuple(args), pathlib.Path(cwd)))
        return self.listing if args[:2] == ["git", "ls-files"] else ""


def listing(python_files, other=0):
    paths = [f"pkg/mod{i}.py" for i in range(python_files)] + [f"docs/page{i}.md" for i in range(other)]
    return "\0".join(paths) + "\0"


def quiet(call, *args, **kwargs):
    with contextlib.redirect_stdout(io.StringIO()) as out, contextlib.redirect_stderr(io.StringIO()):
        result = call(*args, **kwargs)
    return result, out.getvalue()


class ParseUrl(unittest.TestCase):
    def test_accepts_plain_git_and_trailing_slash(self):
        self.assertEqual(add.parse_url("https://github.com/psf/requests"), ("psf", "requests"))
        self.assertEqual(add.parse_url("https://github.com/psf/requests.git"), ("psf", "requests"))
        self.assertEqual(add.parse_url("https://github.com/psf/requests/"), ("psf", "requests"))

    def test_refuses_other_hosts_and_subpaths(self):
        for url in ("https://gitlab.com/a/b", "https://github.com/a", "https://github.com/a/b/tree/main"):
            with self.subTest(url=url), self.assertRaises(add.AddError):
                add.parse_url(url)


class Licenses(unittest.TestCase):
    def test_each_permissive_license_is_accepted(self):
        for spdx in ("MIT", "Apache-2.0", "BSD-2-Clause", "BSD-3-Clause", "ISC", "Unlicense", "0BSD"):
            self.assertEqual(add.check_license(spdx), spdx)

    def test_mpl_and_copyleft_are_refused_with_the_allowlist(self):
        for spdx in ("MPL-2.0", "GPL-3.0-only", "AGPL-3.0-only"):
            with self.subTest(spdx=spdx), self.assertRaisesRegex(add.AddError, "permissive allowlist"):
                add.check_license(spdx)

    def test_missing_or_ambiguous_license_is_refused(self):
        for spdx in ("", "NOASSERTION"):
            with self.subTest(spdx=spdx), self.assertRaisesRegex(add.AddError, "no single SPDX license"):
                add.check_license(spdx)


class Shas(unittest.TestCase):
    def test_full_lowercase_hex_is_valid(self):
        self.assertEqual(add.validate_sha(SHA), SHA)

    def test_short_upper_and_long_shas_are_refused(self):
        for sha in ("0123abc", SHA.upper(), SHA + "0", None, ""):
            with self.subTest(sha=sha), self.assertRaises(add.AddError):
                add.validate_sha(sha)

    def test_default_branch_head_is_resolved(self):
        facts = add.repo_facts(OWNER, REPO, fake_api())
        self.assertEqual(add.resolve_sha(OWNER, REPO, facts, None, fake_api()), SHA)

    def test_given_sha_must_exist(self):
        facts = add.repo_facts(OWNER, REPO, fake_api())
        self.assertEqual(add.resolve_sha(OWNER, REPO, facts, SHA, fake_api()), SHA)
        other = "f" * 40
        with self.assertRaisesRegex(add.AddError, "does not exist"):
            add.resolve_sha(OWNER, REPO, facts, other, fake_api({**ROUTES, f"/repos/{OWNER}/{REPO}/commits/{other}": {"sha": SHA}}))

    def test_warnings_for_archived_and_fork(self):
        facts = {"archived": True, "fork": True}
        self.assertEqual(len(add.warnings_for(facts)), 2)
        self.assertEqual(add.warnings_for({"archived": False, "fork": False}), [])


class Counting(unittest.TestCase):
    def test_counts_only_the_languages_extensions(self):
        paths = ["a.py", "b.PY", "c.pyi", "d.md", "README", "e.pyc", "pkg/f.py"]
        self.assertEqual(add.source_count(paths, ["py", "pyi"]), 4)

    def test_subdirectory_restricts_the_count_by_path_prefix(self):
        paths = ["pkg/a.py", "pkg/sub/b.py", "pkgx/c.py", "d.py"]
        self.assertEqual(add.source_count(paths, ["py"], "pkg"), 2)
        self.assertEqual(add.source_count(paths, ["py"], "pkg/sub/"), 1)

    def test_size_verdict_thresholds(self):
        cases = {0: "error", 9: "error", 10: "warn", 49: "warn", 50: "ok", 300: "ok", 301: "warn"}
        for count, level in cases.items():
            with self.subTest(count=count):
                self.assertEqual(add.size_verdict(count)[0], level)

    def test_tracked_files_splits_nul_separated_listing(self):
        self.assertEqual(add.tracked_files(pathlib.Path("."), FakeGit("a.py\0b/c.py\0")), ["a.py", "b/c.py"])


class Fetch(unittest.TestCase):
    def test_shallow_fetch_of_the_pinned_sha(self):
        with tempfile.TemporaryDirectory() as tmp, mock.patch.object(corpus, "CACHE", pathlib.Path(tmp)):
            git = FakeGit()
            dest = add.fetch_source("widgets", URL, SHA, git)
            commands = [call for call, _ in git.calls]
            self.assertEqual(commands[0], ("git", "init", "-q"))
            self.assertEqual(commands[1], ("git", "remote", "add", "origin", URL))
            self.assertIn(("git", "fetch", "-q", "--depth", "1", "origin", SHA), commands)
            self.assertEqual(commands[-1], ("git", "clean", "-q", "-ffdx"))
            self.assertEqual(dest, pathlib.Path(tmp) / "src" / "widgets")

    def test_existing_checkout_resets_its_origin(self):
        with tempfile.TemporaryDirectory() as tmp, mock.patch.object(corpus, "CACHE", pathlib.Path(tmp)):
            dest = pathlib.Path(tmp) / "src" / "widgets"
            (dest / ".git").mkdir(parents=True)
            git = FakeGit()
            add.fetch_source("widgets", URL, SHA, git)
            self.assertEqual(git.calls[0][0], ("git", "remote", "set-url", "origin", URL))


class Rendering(unittest.TestCase):
    FIELDS = {
        "name": "demo", "languages": ["python"], "status": "unverified", "license": "MIT", "url": URL,
        "sha": SHA, "subdir": "pkg", "indexer": "scip-python", "indexer_version": "0.6.8",
        "setup": "printf '%s\\n' 'a' > /tmp/x\npip install -q -e .",
        "index": "scip-python index --output={out} 'quoted' \\d+",
        "image": "ripwire-corpus-lsp:local",
    }

    def test_rendered_entry_is_valid_toml_with_the_same_values(self):
        block = add.render_entry(self.FIELDS)
        parsed = tomllib.loads(block)["repo"][0]
        self.assertEqual(parsed["setup"], self.FIELDS["setup"])
        self.assertEqual(parsed["index"], self.FIELDS["index"])
        self.assertEqual(parsed["languages"], ["python"])
        self.assertEqual(parsed["subdir"], "pkg")

    def test_value_that_cannot_be_a_literal_string_is_still_valid(self):
        fields = dict(self.FIELDS, setup="echo 'x'\necho '''y''' '")
        parsed = tomllib.loads(add.render_entry(fields))["repo"][0]
        self.assertEqual(parsed["setup"], fields["setup"])

    def test_empty_optional_fields_are_omitted(self):
        block = add.render_entry(dict(self.FIELDS, subdir="", setup="", image=""))
        self.assertNotIn("subdir", block)
        self.assertNotIn("setup", block)
        self.assertNotIn("image", block)

    def test_every_template_renders_a_valid_entry_with_no_placeholder(self):
        table = add.load_languages()
        for language, template in table.items():
            if template["status"] != "template":
                continue
            with self.subTest(language=language):
                block = add.render_entry(add.entry_fields("demo", URL, language, template, "MIT", SHA, ""))
                parsed = corpus.Entry.from_table(tomllib.loads(block)["repo"][0])
                self.assertTrue(parsed.index)
                self.assertNotIn("{project}", block)

    def test_every_template_names_its_languages_extensions(self):
        for language, template in add.load_languages().items():
            with self.subTest(language=language):
                self.assertTrue(template["extensions"])
                if template["status"] == "template":
                    self.assertTrue(template["indexer"])
                if template["status"] in ("blocked", "none"):
                    self.assertTrue(template["reason"])

    def test_every_language_is_listed_once(self):
        self.assertEqual(len(add.load_languages()), 36)


class PlatformRendering(unittest.TestCase):
    def test_platform_is_written_only_when_it_is_not_the_default(self):
        self.assertIn('platform = "linux/arm64"', add.render_entry(dict(Rendering.FIELDS, platform="linux/arm64")))
        self.assertNotIn("platform", add.render_entry(Rendering.FIELDS))

    def test_template_platform_reaches_the_new_entry_only_when_not_the_default(self):
        template = {
            "indexer": "lsp2scip", "indexer_version": "v1", "index": "lsp2scip --out {out}",
            "image": "ripwire-corpus-swift:arm64", "platform": "linux/arm64",
        }
        fields = add.entry_fields("demo", URL, "swift", template, "MIT", SHA, "")
        self.assertEqual(fields["platform"], "linux/arm64")
        plain = add.entry_fields("demo", URL, "python", dict(template, platform="linux/amd64"), "MIT", SHA, "")
        self.assertNotIn("platform", plain)


class Templates(unittest.TestCase):
    def test_scala_template_carries_no_repository_specific_setup(self):
        template = add.load_languages()["scala"]
        self.assertNotIn("setup", template)
        self.assertEqual(template["index"], "cd /src && scip-java index --output={out}")
        self.assertNotIn("scalacheck", "".join(str(value) for value in template.values()))

    def test_php_template_installs_runtime_deps_only_when_a_lock_exists(self):
        setup = add.load_languages()["php"]["setup"]
        self.assertIn("if [ -f composer.lock ]", setup)
        self.assertIn("--no-dev", setup)
        self.assertIn("installed.php", setup)


class Manifest(unittest.TestCase):
    def test_append_keeps_every_existing_byte(self):
        block = add.render_entry(Rendering.FIELDS)
        new = add.append_entry(MANIFEST_TEXT, block)
        self.assertTrue(new.startswith(MANIFEST_TEXT))
        self.assertEqual(len(tomllib.loads(new)["repo"]), 3)
        self.assertEqual(new[len(MANIFEST_TEXT):], "\n" + block)

    def test_append_handles_each_ending(self):
        block = add.render_entry(Rendering.FIELDS)
        for text in (MANIFEST_TEXT, MANIFEST_TEXT.rstrip("\n"), MANIFEST_TEXT + "\n"):
            with self.subTest(ending=repr(text[-3:])):
                new = add.append_entry(text, block)
                self.assertTrue(new.startswith(text))
                self.assertEqual(len(tomllib.loads(new)["repo"]), 3)

    def test_real_manifest_names_and_urls_are_unique(self):
        tables = tomllib.loads(corpus.MANIFEST.read_text())["repo"]
        names = [t["name"] for t in tables]
        urls = [add.normal_url(t["url"]) for t in tables if t.get("url")]
        self.assertEqual(len(names), len(set(names)))
        self.assertEqual(len(urls), len(set(urls)))

    def test_duplicate_name_or_url_is_refused(self):
        entries = add.manifest_entries(MANIFEST_TEXT)
        with self.assertRaisesRegex(add.AddError, "named alpha"):
            add.check_unique(entries, "alpha", "https://github.com/x/y")
        with self.assertRaisesRegex(add.AddError, "as beta"):
            add.check_unique(entries, "gamma", "https://github.com/B/Beta.git/")

    def test_mark_measured_changes_only_the_status_and_adds_floors(self):
        block = add.render_entry(dict(Rendering.FIELDS, status="unverified"))
        text = MANIFEST_TEXT + "\n" + block
        new = add.mark_measured(text, "demo", 0.9860, 0.9160)
        self.assertIn('status = "measured"', new)
        self.assertNotIn('status = "unverified"', new)
        self.assertIn("precision_floor = 0.9840\nrecall_floor = 0.9140", new)
        self.assertTrue(new.startswith(MANIFEST_TEXT))
        self.assertEqual(tomllib.loads(new)["repo"][2]["precision_floor"], 0.984)

    def test_mark_measured_refuses_a_measured_entry(self):
        with self.assertRaisesRegex(add.AddError, "not unverified"):
            add.mark_measured(MANIFEST_TEXT, "alpha", 0.9, 0.9)

    def test_remove_block_leaves_neighbours_byte_identical(self):
        new = add.remove_block(MANIFEST_TEXT, "alpha")
        beta_start = MANIFEST_TEXT.index("[[repo]]\nname = \"beta\"")
        self.assertEqual(new, MANIFEST_TEXT[:MANIFEST_TEXT.index("[[repo]]\nname = \"alpha\"")] + MANIFEST_TEXT[beta_start:])
        self.assertEqual([t["name"] for t in tomllib.loads(new)["repo"]], ["beta"])

    def test_remove_block_of_an_absent_name_changes_nothing(self):
        self.assertEqual(add.remove_block(MANIFEST_TEXT, "nope"), MANIFEST_TEXT)

    def test_remove_block_drops_the_baseline_row_only(self):
        baseline = '# header\n\n[[repo]]\nname = "rich"\nprecision = 0.9\n\n[[repo]]\nname = "dart"\nprecision = 0.8\n'
        new = add.remove_block(baseline, "rich")
        self.assertEqual(new, '# header\n\n[[repo]]\nname = "dart"\nprecision = 0.8\n')


class Floors(unittest.TestCase):
    def test_measured_minus_margin_rounded_down(self):
        self.assertEqual(add.floor_for(0.9860), 0.9840)
        self.assertEqual(add.floor_for(0.98619), 0.9841)

    def test_headroom_is_at_least_the_margin_and_never_above_the_measurement(self):
        for step in range(200, 10001, 37):
            value = step / 10000
            floor = add.floor_for(value)
            with self.subTest(value=value):
                self.assertLessEqual(floor, value)
                self.assertGreaterEqual(value - floor, corpus.MARGIN - 1e-9)

    def test_floor_never_goes_negative(self):
        self.assertEqual(add.floor_for(0.001), 0.0)


class Matrix(unittest.TestCase):
    def test_only_the_language_row_changes(self):
        row = "| python | SCIP indexer (scip-python) | measured | 0.9800-0.9860 | 0.9160 |"
        new, found = add.update_matrix(GATES_TEXT, "python", row)
        self.assertTrue(found)
        old_lines, new_lines = GATES_TEXT.split("\n"), new.split("\n")
        self.assertEqual(len(old_lines), len(new_lines))
        changed = [i for i, (a, b) in enumerate(zip(old_lines, new_lines)) if a != b]
        self.assertEqual(changed, [old_lines.index("| python | SCIP indexer (scip-python) | measured | 0.9860 | 0.9160 |")])

    def test_language_without_a_row_is_left_alone(self):
        self.assertEqual(add.update_matrix(GATES_TEXT, "zig", "| zig |"), (GATES_TEXT, False))

    def test_row_aggregates_measured_entries_and_keeps_the_oracle(self):
        entries = [
            {"name": "a", "languages": ["python"], "status": "measured", "sha": "s", "indexer_version": "v"},
            {"name": "b", "languages": ["python"], "status": "measured", "sha": "t", "indexer_version": "v"},
            {"name": "c", "languages": ["go"], "status": "measured"},
        ]
        numbers = {"a": (0.98, 0.91), "b": (0.99, 0.92), "c": (0.5, 0.5)}
        template = {"oracle": "scip", "indexer": "scip-python"}
        row = add.measured_row("python", template, entries, numbers, GATES_TEXT.split("\n"))
        self.assertEqual(row, "| python | SCIP indexer (scip-python) | measured | 0.9800-0.9900 | 0.9100-0.9200 |")

    def test_row_is_none_without_numbers(self):
        self.assertIsNone(add.measured_row("zig", {"oracle": "lsp", "server": "zls"}, [], {}, []))

    def test_unverified_row_has_dashes(self):
        template = {"oracle": "lsp", "server": "gopls"}
        self.assertEqual(add.unverified_row("go", template), "| go | LSP bridge (gopls) | unverified | - | - |")


class Commands(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        root = pathlib.Path(self.tmp.name)
        self.manifest = root / "corpus.toml"
        self.manifest.write_text(MANIFEST_TEXT)
        self.gates = root / "GATES.md"
        self.gates.write_text(GATES_TEXT)
        self.patches = [
            mock.patch.object(corpus, "MANIFEST", self.manifest),
            mock.patch.object(corpus, "CACHE", root / "cache"),
            mock.patch.object(add, "GATES", self.gates),
            mock.patch.object(add, "BASELINE", root / "baseline.toml"),
        ]
        for patch in self.patches:
            patch.start()

    def tearDown(self):
        for patch in self.patches:
            patch.stop()
        self.tmp.cleanup()

    def run_add(self, *extra, api=None, git=None):
        argv = ["add", URL, "--language", "python", *extra]
        return quiet(add.main, argv, api=api or fake_api(), git=git or FakeGit(listing(60, 3)))

    def test_dry_run_prints_the_entry_and_changes_nothing(self):
        before = self.manifest.read_text()
        code, out = self.run_add("--dry-run")
        self.assertEqual(code, 0)
        self.assertEqual(self.manifest.read_text(), before)
        self.assertIn('status = "unverified"', out)
        self.assertIn("matrix row: | python | SCIP indexer (scip-python) | unverified | - | - |", out)

    def test_add_appends_an_unverified_entry(self):
        code, _ = self.run_add()
        self.assertEqual(code, 0)
        repos = tomllib.loads(self.manifest.read_text())["repo"]
        entry = repos[-1]
        self.assertEqual((entry["name"], entry["status"], entry["license"], entry["sha"]), ("widgets", "unverified", "MIT", SHA))
        self.assertEqual(entry["languages"], ["python"])
        self.assertTrue(self.manifest.read_text().startswith(MANIFEST_TEXT))

    def test_too_few_files_is_a_hard_error(self):
        code, _ = quiet(add.main, ["add", URL, "--language", "python"], api=fake_api(), git=FakeGit(listing(4)))
        self.assertEqual(code, 1)
        self.assertEqual(self.manifest.read_text(), MANIFEST_TEXT)

    def test_out_of_range_file_count_warns_and_still_adds(self):
        stderr = io.StringIO()
        with contextlib.redirect_stdout(io.StringIO()), contextlib.redirect_stderr(stderr):
            code = add.main(["add", URL, "--language", "python", "--dry-run"], api=fake_api(), git=FakeGit(listing(20)))
        self.assertEqual(code, 0)
        self.assertIn("outside the 50-300 guide", stderr.getvalue())
        self.assertIn("--path SUBDIR", stderr.getvalue())

    def test_blocked_and_unknown_languages_are_refused(self):
        code, _ = quiet(add.main, ["add", URL, "--language", "cobol"], api=fake_api(), git=FakeGit(listing(60)))
        self.assertEqual(code, 1)
        code, _ = quiet(add.main, ["add", URL, "--language", "klingon"], api=fake_api(), git=FakeGit(listing(60)))
        self.assertEqual(code, 1)

    def test_duplicate_url_is_refused(self):
        code, _ = quiet(add.main, ["add", "https://github.com/B/Beta", "--language", "go", "--name", "gamma"],
                          api=fake_api(), git=FakeGit(listing(60)))
        self.assertEqual(code, 1)
        self.assertEqual(self.manifest.read_text(), MANIFEST_TEXT)

    def test_dry_run_and_measure_cannot_be_combined(self):
        code, _ = self.run_add("--dry-run", "--measure")
        self.assertEqual(code, 1)

    def test_failed_measure_leaves_the_entry_unverified(self):
        skip = corpus.Skip("docker is not installed")
        with mock.patch.object(corpus, "score_entry", side_effect=skip):
            code, _ = self.run_add("--measure", "--bin", sys.executable)
        self.assertEqual(code, 1)
        entry = tomllib.loads(self.manifest.read_text())["repo"][-1]
        self.assertEqual(entry["status"], "unverified")
        self.assertNotIn("precision_floor", entry)

    def test_successful_measure_writes_floors_and_the_matrix_row(self):
        def score(entry, binary):
            return corpus.Score(entry, 100, 90, 0.9860, 0.9160)
        with mock.patch.object(corpus, "score_entry", side_effect=score):
            code, out = self.run_add("--measure", "--bin", sys.executable)
        self.assertEqual(code, 0)
        entry = tomllib.loads(self.manifest.read_text())["repo"][-1]
        self.assertEqual((entry["status"], entry["precision_floor"], entry["recall_floor"]), ("measured", 0.984, 0.914))
        self.assertIn("| python | SCIP indexer (scip-python) | measured | 0.9860 | 0.9160 |", self.gates.read_text())
        self.assertIn("make verify-ratchet", out)

    def test_list_languages_reports_templates_blocked_and_no_indexer(self):
        code, out = quiet(add.main, ["add", "--list-languages"])
        self.assertEqual(code, 0)
        for heading in ("templates:", "blocked:", "no indexer:"):
            self.assertIn(heading, out)
        self.assertIn("cobol", out.split("blocked:")[1])
        self.assertIn("graphql", out.split("no indexer:")[1])

    def test_remove_deletes_manifest_baseline_and_cache_only_for_that_name(self):
        root = pathlib.Path(corpus.CACHE)
        for path in (root / "src" / "beta", root / "out" / "beta", root / "src" / "beta-keep"):
            path.mkdir(parents=True)
        (root / "index").mkdir(parents=True)
        (root / "index" / f"beta-{'a' * 12}-gopls-0.24.0.scip").write_text("x")
        (root / "index" / f"beta-keep-{'b' * 12}-v.scip").write_text("y")
        add.BASELINE.write_text('[[repo]]\nname = "beta"\nprecision = 0.9\n')
        code, _ = quiet(add.main, ["remove", "beta", "--yes"])
        self.assertEqual(code, 0)
        self.assertNotIn('name = "beta"', self.manifest.read_text())
        self.assertIn('name = "alpha"', self.manifest.read_text())
        self.assertFalse((root / "src" / "beta").exists())
        self.assertFalse((root / "out" / "beta").exists())
        self.assertTrue((root / "src" / "beta-keep").exists())
        self.assertFalse((root / "index" / f"beta-{'a' * 12}-gopls-0.24.0.scip").exists())
        self.assertTrue((root / "index" / f"beta-keep-{'b' * 12}-v.scip").exists())
        self.assertEqual(add.BASELINE.read_text(), "")

    def test_remove_without_yes_and_without_a_terminal_changes_nothing(self):
        before = self.manifest.read_text()
        with mock.patch.object(sys, "stdin", io.StringIO("")):
            code, out = quiet(add.main, ["remove", "beta"])
        self.assertEqual(code, 1)
        self.assertEqual(self.manifest.read_text(), before)
        self.assertIn("aborted", out)

    def test_remove_refuses_a_local_tree_and_unknown_names(self):
        local = MANIFEST_TEXT + '\n[[repo]]\nname = "here"\nlanguages = ["rust"]\nstatus = "measured"\npath = "."\nindexer = "rust-analyzer"\n'
        self.manifest.write_text(local)
        self.assertEqual(quiet(add.main, ["remove", "here", "--yes"])[0], 1)
        self.assertEqual(quiet(add.main, ["remove", "nope", "--yes"])[0], 1)
        self.assertEqual(self.manifest.read_text(), local)


if __name__ == "__main__":
    unittest.main()
