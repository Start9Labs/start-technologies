#!/usr/bin/env python3
import itertools
from pathlib import Path
import sys
import tempfile
import unittest


sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from changelog_version import latest_release, manifest_version, next_version, parse_version


PROJECTS = ("start-os", "start-cli", "start-sdk", "start-tunnel", "start-registry", "start-wrt")


class ParseVersionTests(unittest.TestCase):
    def test_public_parser_normalizes_startos_and_preserves_prerelease(self):
        self.assertEqual(parse_version("start-os", "0.4.1"), ((4, 1, 0), None))
        self.assertEqual(parse_version("start-os", "0.4.0.3-rc.1"), ((4, 0, 3), "rc.1"))
        self.assertEqual(parse_version("start-sdk", "4.0.3+build.1"), ((4, 0, 3), None))

    def test_public_parser_rejects_noncanonical_versions(self):
        for product, version in (("start-os", "9.0.0"), ("start-sdk", "1.2.3.4"),
                                 ("start-sdk", "1.2.3-rc.01"), ("other", "1.2.3")):
            with self.subTest(product=product, version=version), self.assertRaises(ValueError):
                parse_version(product, version)


class NextVersionTests(unittest.TestCase):
    def test_all_tier_combinations_and_orders(self):
        expected = {
            "start-os": {"patch": "0.4.0.4", "minor": "0.4.1", "major": "0.5.0"},
            "start-sdk": {"patch": "4.0.4", "minor": "4.1.0", "major": "5.0.0"},
        }
        for project, results in expected.items():
            baseline = "0.4.0.3" if project == "start-os" else "4.0.3"
            for length in range(1, 5):
                for tiers in itertools.product(("patch", "minor", "major"), repeat=length):
                    tier = "major" if "major" in tiers else "minor" if "minor" in tiers else "patch"
                    with self.subTest(project=project, tiers=tiers):
                        self.assertEqual(next_version(project, baseline, iter(tiers)), results[tier])

    def test_empty_fragments_preserve_exact_version(self):
        for project in PROJECTS:
            versions = ("0.4.0", "0.4.0.0", "0.4.0.3") if project == "start-os" else ("0.0.0", "1.2.3")
            for version in versions:
                with self.subTest(project=project, version=version):
                    self.assertEqual(next_version(project, version, iter(())), version)

    def test_carry_and_reset_boundaries(self):
        cases = (
            ("start-sdk", "0.0.0", ("0.0.1", "0.1.0", "1.0.0")),
            ("start-cli", "9.9.9", ("9.9.10", "9.10.0", "10.0.0")),
            ("start-registry", "1.99.999", ("1.99.1000", "1.100.0", "2.0.0")),
            ("start-tunnel", "99.1.2", ("99.1.3", "99.2.0", "100.0.0")),
            ("start-wrt", "1.2.3", ("1.2.4", "1.3.0", "2.0.0")),
            ("start-os", "0.0.0", ("0.0.0.1", "0.0.1", "0.1.0")),
            ("start-os", "0.4.0", ("0.4.0.1", "0.4.1", "0.5.0")),
            ("start-os", "0.4.0.0", ("0.4.0.1", "0.4.1", "0.5.0")),
            ("start-os", "0.9.9.9", ("0.9.9.10", "0.9.10", "0.10.0")),
            ("start-os", "0.4.99.999", ("0.4.99.1000", "0.4.100", "0.5.0")),
        )
        for project, baseline, results in cases:
            for tier, result in zip(("patch", "minor", "major"), results):
                with self.subTest(project=project, baseline=baseline, tier=tier):
                    self.assertEqual(next_version(project, baseline, [tier]), result)

    def test_invalid_tier_even_after_major(self):
        for tier in ("", "PATCH", "security", "minor ", None, 1):
            with self.subTest(tier=tier), self.assertRaises(ValueError):
                next_version("start-sdk", "1.2.3", ["major", tier])

    def test_malformed_versions(self):
        invalid = ("", "1", "1.2", "v1.2.3", "1.2.3.4", "01.2.3", "1.02.3",
                   "1.2.03", "-1.2.3", "1.2.3\n", " 1.2.3", "1.2.3-", "1.2.3+",
                   "1.2.3-alpha.01", "1.2.3-a_1", "1.2.3+a_1", None, 123)
        for project in PROJECTS:
            values = invalid + (("0.4", "0.04.0", "0.4.0.03", "0.4.0.1.2", "1.4.0", "1.4.0.3")
                                if project == "start-os" else ())
            for value in values:
                with self.subTest(project=project, value=value), self.assertRaises(ValueError):
                    next_version(project, value, [])

    def test_prereleases_are_not_baselines(self):
        for project, value in (("start-sdk", "1.2.3-beta.1"), ("start-os", "0.4.0.3-rc.1")):
            with self.subTest(project=project), self.assertRaises(ValueError):
                next_version(project, value, [])

    def test_build_metadata_is_stable(self):
        self.assertEqual(next_version("start-sdk", "1.2.3+build.1", []), "1.2.3+build.1")
        self.assertEqual(next_version("start-sdk", "1.2.3+build.1", ["patch"]), "1.2.4")


class LatestReleaseTests(unittest.TestCase):
    def test_numeric_order_independent_of_tag_order(self):
        tags = ["start-sdk/v9.99.99", "start-sdk/v10.0.0", "start-sdk/v9.100.0", "start-sdk/v2.0.0"]
        for ordered in itertools.permutations(tags):
            self.assertEqual(latest_release("start-sdk", iter(ordered)), "10.0.0")
        self.assertEqual(latest_release("start-sdk", ["start-sdk/v1.9.9", "start-sdk/v1.10.0"]), "1.10.0")
        self.assertEqual(latest_release("start-sdk", ["start-sdk/v1.2.9", "start-sdk/v1.2.10"]), "1.2.10")

    def test_exact_namespace_and_stable_tags(self):
        for project in PROJECTS:
            version = "0.4.0.3" if project == "start-os" else "1.2.3"
            tags = [
                "v99.0.0", "other/v99.0.0", f"{project}-extra/v99.0.0",
                f"refs/tags/{project}/v99.0.0", f"{project}/99.0.0", f"{project}/V99.0.0",
                f"{project}/v{version}-alpha.1", f"{project}/v{version}-beta.9",
                f"{project}/v{version}-rc.1", f"{project}/v{version}/extra",
                f"{project}/v01.2.3", f"{project}/v1.2.3.4.5", f"{project}/v{version}",
            ]
            with self.subTest(project=project):
                self.assertEqual(latest_release(project, tags), version)

    def test_prereleases_do_not_override_stable(self):
        self.assertEqual(latest_release("start-wrt", ["start-wrt/v1.0.0", "start-wrt/v2.0.0-beta.4"]), "1.0.0")
        self.assertEqual(latest_release("start-os", ["start-os/v0.4.0.3", "start-os/v0.5.0-rc.1"]), "0.4.0.3")

    def test_no_stable_release(self):
        for tags in ([], ["start-sdk/v1.0.0-beta.1"], ["other/v1.0.0"], ["start-sdk/vbad"], ["start-sdk/v1.2.3.4"]):
            with self.subTest(tags=tags):
                self.assertIsNone(latest_release("start-sdk", tags))

    def test_startos_normalizes_omitted_patch_for_comparison(self):
        self.assertEqual(latest_release("start-os", ["start-os/v0.4.0.99", "start-os/v0.4.1"]), "0.4.1")
        self.assertEqual(latest_release("start-os", ["start-os/v0.4.0", "start-os/v0.4.0.1"]), "0.4.0.1")
        self.assertEqual(latest_release("start-os", ["start-os/v0.9.99.99", "start-os/v0.10.0"]), "0.10.0")
        self.assertIsNone(latest_release("start-os", ["start-os/v1.2.3.4", "start-os/v0.4.0.01"]))

    def test_build_metadata_is_stable(self):
        self.assertEqual(latest_release("start-sdk", ["start-sdk/v1.2.3+build.4"]), "1.2.3+build.4")


class ManifestVersionTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)

    def write(self, path, text):
        target = self.root / path
        target.parent.mkdir(parents=True, exist_ok=True)
        target.write_text(text, encoding="utf-8")

    def test_canonical_manifest_paths(self):
        self.write("package.json", '{"version":"0.4.0.3"}')
        self.write("projects/start-os/Cargo.toml", '[package]\nversion = "0.4.0-rev.3"')
        self.write("projects/start-os/package.json", '{"version":"9.9.9"}')
        self.write("projects/start-sdk/package.json", '{"version":"3.0.3"}')
        self.write("projects/start-sdk/Cargo.toml", '[package]\nversion = "9.9.9"')
        for project in ("start-cli", "start-tunnel", "start-registry"):
            self.write(f"projects/{project}/Cargo.toml", '[package]\nversion = "1.2.3" # VERSION_BUMP\n')
            self.assertEqual(manifest_version(self.root, project), "1.2.3")
        self.write("projects/start-wrt/Cargo.toml", '[package]\nversion = "9.9.9"')
        self.write("projects/start-wrt/backend/ctrl/Cargo.toml", '[package]\nversion = "1.3.0"')
        self.assertEqual(manifest_version(self.root, "start-os"), "0.4.0.3")
        self.assertEqual(manifest_version(self.root, "start-sdk"), "3.0.3")
        self.assertEqual(manifest_version(self.root, "start-wrt"), "1.3.0")

    def test_package_table_only_and_literal_quotes(self):
        self.write("projects/start-cli/Cargo.toml", '''[dependencies]
version = "9.9.9" # VERSION_BUMP
 [ package ] # manifest
name = "start-cli"
  version = '1.2.3-beta.1+build' # VERSION_BUMP
[[bin]]
version = "8.8.8"
[package.metadata]
version = "7.7.7"
''')
        self.assertEqual(manifest_version(self.root, "start-cli"), "1.2.3-beta.1+build")

    def test_malformed_or_missing_package_version(self):
        for text in ('[dependencies]\nversion = "1.2.3"', '[package]\nname = "cli"',
                     '[package]\nversion.workspace = true', '[package]\nversion = 123',
                     '[package]\nversion = "1.2.3"\nversion = "1.2.4"',
                     '[package]\nversion = "1.2.3.4"', '[package]\nversion = "1.2.03"',
                     '[package]\nversion = "1.2.3-alpha.01"'):
            self.write("projects/start-cli/Cargo.toml", text)
            with self.subTest(text=text), self.assertRaises(ValueError):
                manifest_version(self.root, "start-cli")

    def test_invalid_json_version(self):
        for text in ('{}', '[]', '{"version":null}', '{"version":123}', '{"version":"1.2.3.4"}', '{'):
            self.write("projects/start-sdk/package.json", text)
            with self.subTest(text=text), self.assertRaises(ValueError):
                manifest_version(self.root, "start-sdk")

    def test_missing_manifest(self):
        with self.assertRaises(FileNotFoundError):
            manifest_version(self.root, "start-wrt")

    def test_unknown_project(self):
        for project in ("other", "../start-os", "projects/start-sdk", ""):
            with self.subTest(project=project):
                with self.assertRaises(ValueError):
                    manifest_version(self.root, project)
                with self.assertRaises(ValueError):
                    next_version(project, "1.2.3", [])
                with self.assertRaises(ValueError):
                    latest_release(project, [])


if __name__ == "__main__":
    unittest.main()
