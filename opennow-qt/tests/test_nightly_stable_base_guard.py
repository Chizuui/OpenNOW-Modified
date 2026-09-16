import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import textwrap
import unittest


sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "packaging"))
from nightly_release import nightly_version, validate_nightly_base, version_base
from sign_nightly_release import published_stable_version


ROOT = Path(__file__).resolve().parents[2]
WORKFLOW = ROOT / ".github/workflows/qt-ci.yml"
GUARD_STEP = "Require a nightly base newer than the published stable"

START_NIGHTLY = "1.0.0-nightly.620.1"
PUBLISHED_STABLE = "1.0.1"
CORRECTED_NIGHTLY = "1.0.2-nightly.620.1"
STABLE_TAG = "v1.0.1"
HISTORICAL_TAGS = ("nightly-60030b626b57914e9e4d538aba1d4ed954a82653",
                   "nightly-529d0b076e620e64e1f29cc7ad73cc1cb4ed0ebc")
SCRIPT = "opennow-qt/packaging/sign_nightly_release.py"


def jobs(workflow):
    sections = re.split(r"^  ([a-z][a-z-]*):\n", workflow.split("jobs:\n", 1)[1], flags=re.MULTILINE)
    return dict(zip(sections[1::2], sections[2::2]))


def step(job, name):
    return job.split(f"      - name: {name}\n", 1)[1].split("      - name:", 1)[0]


def run_script(step_text):
    return textwrap.dedent(step_text.split("        run: |\n", 1)[1])


def release(tag, prerelease=False, draft=False):
    return {
        "tag_name": tag, "draft": draft, "prerelease": prerelease,
        "name": f"OpenNOW {tag}", "id": 1, "assets": [],
    }


class NightlyBaseTest(unittest.TestCase):
    def test_nightly_and_stable_bases_share_one_parser(self):
        self.assertEqual(version_base(CORRECTED_NIGHTLY), (1, 0, 2))
        self.assertEqual(version_base(PUBLISHED_STABLE), (1, 0, 1))
        self.assertEqual(version_base("v1.0.2"), (1, 0, 2))
        self.assertEqual(version_base("1.1.0+build.1"), (1, 1, 0))
        self.assertEqual(version_base("1.1.0+build-01.x"), (1, 1, 0))
        self.assertEqual(version_base("0.5.3-supporter.256.1"), (0, 5, 3))

    def test_invalid_bases_fail_closed(self):
        for version in (HISTORICAL_TAGS[0], "1.0", "v", "", "1.0.0.1", "01.0.0", "1.0.0;bad"):
            with self.subTest(version=version):
                with self.assertRaises(ValueError):
                    version_base(version)

    def test_invalid_build_metadata_fails_closed(self):
        for version in ("1.1.0+", "1.1.0+..", "1.1.0+build..1", "1.1.0+.build", "1.1.0+build."):
            with self.subTest(version=version):
                with self.assertRaises(ValueError):
                    version_base(version)

    def test_only_generated_nightly_versions_pass_the_guard(self):
        for version in ("1.0.2", "1.0.0-nightly", "1.0.0-nightly.1.0", "1.0.0-nightly.01.1",
                        "1.0.0-supporter.620.1", HISTORICAL_TAGS[1], "invalid"):
            with self.subTest(version=version):
                with self.assertRaisesRegex(ValueError, "Invalid nightly version"):
                    validate_nightly_base(version, PUBLISHED_STABLE)

    def test_equal_or_lower_stable_bases_are_rejected(self):
        for version, stable in ((START_NIGHTLY, PUBLISHED_STABLE), (START_NIGHTLY, "1.0.0"),
                                ("1.0.1-nightly.620.1", PUBLISHED_STABLE),
                                (CORRECTED_NIGHTLY, "1.0.2"), (CORRECTED_NIGHTLY, "1.0.3"),
                                (CORRECTED_NIGHTLY, "1.1.0")):
            with self.subTest(version=version, stable=stable):
                with self.assertRaisesRegex(ValueError, "must be newer than the published stable"):
                    validate_nightly_base(version, stable)

    def test_higher_stable_bases_are_accepted(self):
        self.assertEqual(validate_nightly_base(CORRECTED_NIGHTLY, PUBLISHED_STABLE), (1, 0, 2))
        self.assertEqual(validate_nightly_base("1.0.0-nightly.413.1", "0.5.5"), (1, 0, 0))

    def test_bootstrap_still_validates_the_nightly_identity(self):
        self.assertEqual(validate_nightly_base(CORRECTED_NIGHTLY), (1, 0, 2))
        for version in ("1.0.2", "1.0.0-nightly.1.0"):
            with self.subTest(version=version):
                with self.assertRaisesRegex(ValueError, "Invalid nightly version"):
                    validate_nightly_base(version)


class PublishedStableTest(unittest.TestCase):
    def test_highest_published_stable_wins(self):
        pages = [[release("v1.0.1"), release("v1.0.0"), release("v0.5.5")], [release("v0.5.4")]]
        self.assertEqual(published_stable_version(pages), PUBLISHED_STABLE)

    def test_build_metadata_keeps_stable_precedence(self):
        pages = [[release("v1.0.1"), release("v1.1.0+build.1")]]
        self.assertEqual(published_stable_version(pages), "1.1.0")
        with self.assertRaisesRegex(ValueError, "must be newer than the published stable"):
            validate_nightly_base(CORRECTED_NIGHTLY, published_stable_version(pages))

    def test_invalid_build_metadata_tags_are_ignored(self):
        for tag in ("v9.0.0+", "v9.0.0+..", "v9.0.0+build..1", "v9.0.0+.build", "v9.0.0+build."):
            with self.subTest(tag=tag):
                self.assertEqual(published_stable_version([[release(tag)]]), None)
                self.assertEqual(
                    published_stable_version([[release(tag), release(STABLE_TAG)]]), PUBLISHED_STABLE)

    def test_numeric_ordering_spans_unordered_pages(self):
        pages = [[release("v1.9.99"), release("v1.2.0")], [release("v1.10.0"), release("v1.9.100")]]
        self.assertEqual(published_stable_version(pages), "1.10.0")
        self.assertEqual(validate_nightly_base("1.10.0-nightly.620.1", "1.9.99"), (1, 10, 0))

    def test_drafts_prereleases_and_non_versions_are_ignored(self):
        pages = [[release("v1.0.1", draft=True), release("v1.0.2", prerelease=True),
                  release("v1.0.3", prerelease=True, draft=True), release("v2.0.0", prerelease=True),
                  release(START_NIGHTLY), release("v1.1.0-rc.1+build.1"),
                  release(HISTORICAL_TAGS[0]), release(HISTORICAL_TAGS[1]), release("v0.5.5")]]
        self.assertEqual(published_stable_version(pages), "0.5.5")

    def test_no_published_stable_is_a_valid_answer(self):
        self.assertIsNone(published_stable_version([[]]))
        self.assertIsNone(published_stable_version([[release(HISTORICAL_TAGS[0]), release("v1.0.2-beta.1")]]))

    def test_malformed_metadata_fails_closed(self):
        for pages in ({}, [], [{}], [release("v1.0.1")], [[{"tag_name": STABLE_TAG}]],
                      [[{"tag_name": 1, "draft": False, "prerelease": False}]],
                      [[{"tag_name": STABLE_TAG, "draft": "false", "prerelease": False}]],
                      [[{"tag_name": STABLE_TAG, "draft": False}]], ["v1.0.1"], [[None]]):
            with self.subTest(pages=pages):
                with self.assertRaises(ValueError):
                    published_stable_version(pages)


class ReleaseGuardCliTest(unittest.TestCase):
    def guard(self, version, payload):
        with tempfile.TemporaryDirectory() as directory:
            releases = Path(directory) / "releases.json"
            releases.write_text(payload if isinstance(payload, str) else json.dumps(payload))
            return subprocess.run(
                [sys.executable, str(ROOT / SCRIPT), "release-guard",
                 "--version", version, "--releases", str(releases)],
                capture_output=True, text=True)

    def test_published_stable_must_be_exceeded(self):
        for version, stable, expected in ((START_NIGHTLY, PUBLISHED_STABLE, False),
                                          (CORRECTED_NIGHTLY, PUBLISHED_STABLE, True),
                                          (CORRECTED_NIGHTLY, "1.0.2", False),
                                          (CORRECTED_NIGHTLY, "1.1.0+build.1", False),
                                          ("1.0.3-nightly.700.2", PUBLISHED_STABLE, True)):
            with self.subTest(version=version, stable=stable):
                result = self.guard(version, [[release(f"v{stable}")]])
                self.assertEqual(result.returncode == 0, expected, result.stdout + result.stderr)

    def test_invalid_build_metadata_tags_cannot_hijack_the_stable_base(self):
        pages = [[release("v9.0.0+build..1"), release(STABLE_TAG), release("v9.0.0+")]]
        self.assertEqual(published_stable_version(pages), PUBLISHED_STABLE)
        result = self.guard(CORRECTED_NIGHTLY, pages)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_first_nightly_without_a_published_stable_is_allowed(self):
        for payload in ([[]], [[release(HISTORICAL_TAGS[1])]], [[release("v1.0.2-beta.1")]]):
            with self.subTest(payload=payload):
                result = self.guard(CORRECTED_NIGHTLY, payload)
                self.assertEqual(result.returncode, 0, result.stdout + result.stderr)

    def test_non_nightly_versions_fail_closed_even_without_stable_history(self):
        for version in ("1.0.2", "1.0.0-nightly", HISTORICAL_TAGS[0]):
            with self.subTest(version=version):
                self.assertNotEqual(self.guard(version, [[]]).returncode, 0)

    def test_missing_or_malformed_payloads_fail_closed(self):
        cases = ("[not json]", "{}", "[]", '[["v1.0.1"]]', '[[{"tag_name": "v1.0.1"}]]')
        for payload in cases:
            with self.subTest(payload=payload):
                self.assertNotEqual(self.guard(CORRECTED_NIGHTLY, payload).returncode, 0)
        with tempfile.TemporaryDirectory() as directory:
            result = subprocess.run(
                [sys.executable, str(ROOT / SCRIPT), "release-guard",
                 "--version", CORRECTED_NIGHTLY, "--releases", str(Path(directory) / "absent.json")],
                capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)

    def test_stable_publication_path_ignores_the_guard(self):
        self.assertNotIn("release-guard", (ROOT / ".github/workflows/qt-stable-release.yml").read_text())
        self.assertEqual(WORKFLOW.read_text().count("release-guard"), 2)


class WorkflowGuardTest(unittest.TestCase):
    def setUp(self):
        self.workflow = WORKFLOW.read_text()
        self.entries = jobs(self.workflow)
        self.preflight = step(self.entries["preflight"], GUARD_STEP)
        self.publisher = self.entries["publish-nightly"]
        self.publish_guard = step(self.publisher, GUARD_STEP)

    def test_nightly_publication_requires_the_base_guard(self):
        self.assertIn("if: inputs.publish_nightly\n", self.preflight)
        self.assertIn("--releases \"$RUNNER_TEMP/releases.json\"", self.preflight)
        self.assertIn("--releases \"$RUNNER_TEMP/releases.json\"", self.publish_guard)
        self.assertIn("    needs: [contracts, preflight]\n", self.entries["build"])

    def test_guard_precedes_the_undraft_operation(self):
        order = [self.publisher.index(name) for name in (
            "      - name: Verify signed inventory\n",
            "      - name: Upload the complete draft release\n",
            f"      - name: {GUARD_STEP}\n",
            "      - name: Publish the verified draft\n",
        )]
        self.assertEqual(order, sorted(order))
        self.assertIn("--draft=false --prerelease --latest=false",
                      step(self.publisher, "Publish the verified draft"))
        self.assertIn("--draft --prerelease --latest=false",
                      step(self.publisher, "Upload the complete draft release"))
        self.assertNotIn("--draft=false", self.publish_guard)
        self.assertNotIn("release create", self.publish_guard)
        self.assertNotIn("release edit", self.publish_guard)

    def test_guard_metadata_is_not_caller_supplied(self):
        for name, entry in (("preflight", self.preflight), ("publish-nightly", self.publish_guard)):
            with self.subTest(job=name):
                script = run_script(entry)
                self.assertIn("gh api --paginate --slurp", script)
                self.assertIn(f"python3 {SCRIPT} release-guard", script)
                self.assertNotIn("inputs.", script)
                self.assertNotIn("${{", script)

    def guarded_step(self, entry, metadata, **env):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            tools = root / "tools"
            tools.mkdir()
            fixture = root / "source.json"
            fixture.write_text(metadata if isinstance(metadata, str) else json.dumps(metadata))
            argv = root / "argv.txt"
            gh = tools / "gh"
            gh.write_text(
                "#!/bin/sh\n"
                "printf '%s\\n' \"$*\" > \"$GH_ARGV\"\n"
                "cat \"$GH_FIXTURE\"\n"
                "exit \"${GH_FIXTURE_EXIT:-0}\"\n")
            gh.chmod(0o700)
            result = subprocess.run(
                ["bash", "-euo", "pipefail", "-c", run_script(entry)],
                cwd=ROOT,
                env={
                    **os.environ,
                    "PATH": str(tools) + os.pathsep + os.environ["PATH"],
                    "RUNNER_TEMP": str(root),
                    "GH_FIXTURE": str(fixture),
                    "GH_ARGV": str(argv),
                    "GITHUB_REPOSITORY": "OpenCloudGaming/OpenNOW",
                    **env,
                },
                capture_output=True, text=True)
            return result, argv.read_text()

    def project_base(self):
        return version_base(nightly_version(ROOT / "opennow-qt/CMakeLists.txt", 1, 1, channel="stable"))

    def previous_base(self):
        major, minor, patch = self.project_base()
        if patch:
            return (major, minor, patch - 1)
        return (major, minor - 1, 0) if minor else None

    def test_early_guard_bounds_the_run_identity_version(self):
        previous = self.previous_base()
        if previous is not None:
            result, argv = self.guarded_step(
                self.preflight, [[release(".".join(map(str, previous)))]],
                GITHUB_RUN_NUMBER="620", GITHUB_RUN_ATTEMPT="1")
            self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
            self.assertIn("--paginate --slurp", argv)
        result, _ = self.guarded_step(
            self.preflight, [[release(".".join(map(str, self.project_base())))]],
            GITHUB_RUN_NUMBER="620", GITHUB_RUN_ATTEMPT="1")
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("must be newer than the published stable", result.stderr)

    def test_late_guard_rejects_the_mistaken_and_released_nightly_bases(self):
        for version in (START_NIGHTLY, "1.0.1-nightly.620.1"):
            with self.subTest(version=version):
                result, _ = self.guarded_step(
                    self.publish_guard, [[release(STABLE_TAG)]], RELEASE_VERSION=version)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("must be newer than the published stable", result.stderr)

    def test_late_guard_accepts_the_corrected_base_and_tolerates_history(self):
        metadata = [[release(HISTORICAL_TAGS[0]), release("v1.0.0")], [release(STABLE_TAG)]]
        result, argv = self.guarded_step(self.publish_guard, metadata, RELEASE_VERSION=CORRECTED_NIGHTLY)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertIn("--paginate --slurp", argv)

    def test_late_guard_fails_closed_when_metadata_is_unavailable(self):
        result, _ = self.guarded_step(
            self.publish_guard, [[release(STABLE_TAG)]], RELEASE_VERSION=CORRECTED_NIGHTLY, GH_FIXTURE_EXIT="1")
        self.assertNotEqual(result.returncode, 0)
        result, _ = self.guarded_step(self.publish_guard, "[not json]", RELEASE_VERSION=CORRECTED_NIGHTLY)
        self.assertNotEqual(result.returncode, 0)


if __name__ == "__main__":
    unittest.main()
