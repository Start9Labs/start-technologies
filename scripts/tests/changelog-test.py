#!/usr/bin/env python3
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "changelog.py"
PRODUCT = "projects/start-os"
HISTORY = b"# Changelog\r\n\r\n## [0.3.0]\r\n\r\n### Fixed\r\n\r\n- Historical bytes.\r\n"


class ChangelogTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.env = dict(os.environ, GIT_CONFIG_NOSYSTEM="1", GIT_CONFIG_COUNT="0")
        self.git("init", "-q", "-b", "master")
        self.git("config", "user.name", "Fixture")
        self.git("config", "user.email", "fixture@example.com")
        self.git("config", "commit.gpgsign", "false")
        self.git("config", "tag.gpgsign", "false")
        self.git("config", "core.hooksPath", "/dev/null")
        self.git("remote", "add", "origin", str(self.root))
        self.product = self.root / PRODUCT
        self.fragments = self.product / "changelog"
        self.fragments.mkdir(parents=True)
        self.history = self.product / "CHANGELOG.md"
        self.history.write_bytes(HISTORY)
        self.commit()

    def git(self, *args, env=None):
        return subprocess.check_output(
            ["git", *args], cwd=self.root, env=env or self.env, stderr=subprocess.PIPE
        ).decode().strip()

    def commit(self, date=None):
        self.git("add", ".")
        env = dict(self.env, GIT_AUTHOR_DATE=date, GIT_COMMITTER_DATE=date) if date else None
        self.git("commit", "--allow-empty", "-qm", "fixture", env=env)
        return self.git("rev-parse", "HEAD")

    def tag(self, version, annotated=False):
        args = ("-a", "-m", "fixture") if annotated else ()
        self.git("tag", *args, f"start-os/v{version}")
        return f"start-os/v{version}"

    def fragment(self, name="patch-fixed-example.md", body="- Fixed example.\n"):
        self.fragments.mkdir(parents=True, exist_ok=True)
        path = self.fragments / name
        path.write_text(body)
        return path

    def run_script(self, *args, success=True):
        result = subprocess.run(
            [sys.executable, str(SCRIPT), *args], cwd=self.root, env=self.env,
            capture_output=True,
        )
        if success:
            self.assertEqual(result.returncode, 0, result.stderr.decode())
        else:
            self.assertNotEqual(result.returncode, 0)
        return result

    def render(self, version="0.4.0.1", ref=None):
        args = ("--ref", ref) if ref else ()
        return self.run_script("render", PRODUCT, version, *args).stdout

    def test_deterministic_grouping_and_multiline_bullets(self):
        self.fragment("major-security-z.md", "- Security.\n")
        self.fragment("patch-added-z.md", "- Zed.\n  Continued **Markdown**.\n\n- Second.\n")
        self.fragment("minor-added-a.md", "* Alpha.\n")
        self.fragment("patch-removed-a.md", "- Removed.\n")
        self.fragment("patch-deprecated-a.md", "- Deprecated.\n")
        self.fragment("patch-changed-a.md", "- Changed.\n")
        self.fragment("patch-fixed-a.md", "- Fixed.\n")
        expected = (
            b"# Changelog\r\n\r\n## [0.4.0.1]\n\n"
            b"### Added\n\n* Alpha.\n\n- Zed.\n  Continued **Markdown**.\n\n- Second.\n\n"
            b"### Changed\n\n- Changed.\n\n### Deprecated\n\n- Deprecated.\n\n"
            b"### Removed\n\n- Removed.\n\n### Fixed\n\n- Fixed.\n\n"
            b"### Security\n\n- Security.\n\n"
            + HISTORY[HISTORY.index(b"## "):]
        )
        self.assertEqual(self.render(), expected)
        self.assertEqual(self.render(), expected)

    def test_invalid_filenames(self):
        for name in ("fixed-name.md", "patch-Fixed-name.md", "patch-other-name.md",
                     "patch-fixed-.md", "patch-fixed-name.txt", "patch-fixed-name space.md"):
            with self.subTest(name=name):
                path = self.fragment(name)
                self.run_script("validate", PRODUCT, success=False)
                path.unlink()

    def test_rejected_bodies(self):
        for body in ("", " \n", "plain text", "- Item.\n\n## Heading\n",
                     "- Item.\nHeading\n=======\n", "- Item.\n  ### Heading\n"):
            with self.subTest(body=body):
                self.fragment(body=body)
                self.run_script("validate", PRODUCT, success=False)

    def test_empty_directory_is_valid(self):
        self.run_script("validate", PRODUCT)
        self.assertEqual(self.render(), HISTORY)

    def test_symlink_and_nested_directory_rejected(self):
        target = self.root / "outside.md"
        target.write_text("- Outside.\n")
        path = self.fragments / "patch-fixed-link.md"
        path.symlink_to(target)
        self.run_script("validate", PRODUCT, success=False)
        self.commit()
        tag = self.tag("0.4.0.1")
        self.run_script("render", PRODUCT, "0.4.0.1", "--ref", tag, success=False)
        path.unlink()
        path.mkdir()
        self.run_script("validate", PRODUCT, success=False)
        path.rmdir()
        self.fragments.rmdir()
        self.fragments.symlink_to(self.root, target_is_directory=True)
        self.run_script("validate", PRODUCT, success=False)

    def test_version_injection_rejected_and_prerelease_accepted(self):
        self.fragment()
        self.render("0.4.0.1-rc.2+build.1")
        for version in ("1.2", "1.2.3\n## Injected", "[1.2.3]", "1.2.3/evil"):
            self.run_script("render", PRODUCT, version, success=False)

    def test_render_reads_tag_not_newer_master(self):
        original = self.fragment(body="- Tagged.\n")
        self.commit()
        tag = self.tag("0.4.0.1", annotated=True)
        expected = self.render(ref=tag)
        original.write_text("- Modified on master.\n")
        self.fragment("minor-added-new.md", "- New on master.\n")
        self.history.write_text("# Completely different master history\n")
        self.commit()
        self.assertEqual(self.render(ref=tag), expected)
        self.assertNotIn(b"master", expected)
        self.assertIn(b"- Tagged.", expected)

    def test_sync_retains_modified_and_new_fragments_and_retries(self):
        unchanged = self.fragment()
        modified = self.fragment("minor-added-modified.md", "- Tagged added.\n")
        self.commit()
        tag = self.tag("0.4.0.1")
        expected = self.render(ref=tag)
        modified.write_text("- Revised after tag.\n")
        new = self.fragment("patch-fixed-new.md", "- New after tag.\n")
        self.commit()
        self.run_script("sync")
        self.assertEqual(self.history.read_bytes(), expected)
        self.assertFalse(unchanged.exists())
        self.assertEqual(modified.read_text(), "- Revised after tag.\n")
        self.assertTrue(new.exists())
        self.run_script("sync")
        self.assertEqual(self.history.read_bytes(), expected)
        self.assertTrue(modified.exists())
        self.assertTrue(new.exists())

    def test_overlapping_snapshots_and_pending_ancestor_sections(self):
        first = self.fragment(body="- First release.\n")
        self.commit()
        self.tag("0.4.0.1")
        self.fragment("minor-added-next.md", "- Second release.\n")
        self.commit()
        second_tag = self.tag("0.4.0.2")
        expected = self.render("0.4.0.2", second_tag)
        self.assertEqual(expected.count(b"- First release."), 1)
        self.assertEqual(expected.count(b"- Second release."), 1)
        self.assertLess(expected.index(b"## [0.4.0.2]"), expected.index(b"## [0.4.0.1]"))
        self.run_script("sync")
        self.assertEqual(self.history.read_bytes(), expected)
        self.assertFalse(first.exists())
        self.assertEqual(list(self.fragments.iterdir()), [])
        self.run_script("sync")
        self.assertEqual(self.history.read_bytes(), expected)

    def test_topology_not_semver_or_commit_time_orders_tags(self):
        self.fragment(body="- Ancestor.\n")
        self.commit("2026-01-02T00:00:00Z")
        self.tag("0.9.0")
        self.fragment("patch-fixed-descendant.md", "- Descendant.\n")
        self.commit("2026-01-01T00:00:00Z")
        tag = self.tag("0.1.0")
        expected = self.render("0.1.0", tag)
        self.run_script("sync")
        self.assertEqual(self.history.read_bytes(), expected)
        self.assertLess(expected.index(b"## [0.1.0]"), expected.index(b"## [0.9.0]"))

    def test_unmerged_tag_excluded(self):
        self.git("checkout", "-qb", "unmerged")
        self.fragment(body="- Unmerged.\n")
        self.commit()
        self.tag("0.99.0")
        self.git("checkout", "-q", "master")
        self.fragment(body="- Merged.\n")
        self.commit()
        tag = self.tag("0.4.0.1")
        rendered = self.render(ref=tag)
        self.run_script("sync")
        self.assertEqual(self.history.read_bytes(), rendered)
        self.assertNotIn(b"Unmerged", rendered)
        self.assertNotIn(b"0.99.0", rendered)

    def test_legacy_tag_and_existing_version_heading_are_preserved(self):
        self.tag("0.3.0")
        self.run_script("sync")
        self.assertEqual(self.history.read_bytes(), HISTORY)
        self.assertEqual(self.render("0.3.0", "start-os/v0.3.0"), HISTORY)
        fragment = self.fragment()
        existing = self.render()
        self.history.write_bytes(existing)
        self.commit()
        tag = self.tag("0.4.0.1")
        self.assertEqual(self.render(ref=tag), existing)
        self.run_script("sync")
        self.assertEqual(self.history.read_bytes(), existing)
        self.assertFalse(fragment.exists())
        self.run_script("sync")
        self.assertEqual(self.history.read_bytes(), existing)

    def test_moved_processed_tag_does_not_consume_updated_fragment(self):
        fragment = self.fragment()
        self.commit()
        self.tag("0.4.0.1")
        self.run_script("sync")
        history = self.history.read_bytes()
        self.commit()
        fragment = self.fragment(body="- Replacement release text.\n")
        self.commit()
        self.git("tag", "-f", "start-os/v0.4.0.1")
        self.run_script("sync", success=False)
        self.run_script("render", PRODUCT, "0.4.0.1", "--ref", "start-os/v0.4.0.1", success=False)
        self.assertEqual(self.history.read_bytes(), history)
        self.assertTrue(fragment.exists())

    def test_reintroduced_identical_fragment_belongs_to_next_release(self):
        fragment = self.fragment()
        self.commit()
        self.tag("0.4.0.1")
        self.run_script("sync")
        self.commit()
        fragment = self.fragment()
        self.run_script("check-version", PRODUCT, "0.4.0.2")
        self.run_script("sync")
        self.assertTrue(fragment.exists())
        self.commit()
        tag = self.tag("0.4.0.2")
        rendered = self.render("0.4.0.2", tag)
        self.assertEqual(rendered.count(b"- Fixed example."), 2)
        self.run_script("sync")
        self.assertEqual(self.history.read_bytes(), rendered)
        self.assertFalse(fragment.exists())

    def test_modified_same_path_at_next_tag_is_new_ownership(self):
        fragment = self.fragment(body="- Old text.\n")
        self.commit()
        self.tag("0.4.0.1")
        fragment.write_text("- New text.\n")
        self.commit()
        tag = self.tag("0.4.0.2")
        rendered = self.render("0.4.0.2", tag)
        self.assertEqual(rendered.count(b"- Old text."), 1)
        self.assertEqual(rendered.count(b"- New text."), 1)
        self.run_script("sync")
        self.assertEqual(self.history.read_bytes(), rendered)
        self.assertFalse(fragment.exists())

    def test_later_sync_reuses_committed_version_markers(self):
        self.fragment(body="- First release.\n")
        self.commit()
        self.tag("0.4.0.1")
        self.run_script("sync")
        first_history = self.history.read_bytes()
        self.commit()
        self.run_script("sync")
        self.assertEqual(self.history.read_bytes(), first_history)
        self.fragment("minor-added-next.md", "- Second release.\n")
        self.commit()
        tag = self.tag("0.4.0.2")
        expected = self.render("0.4.0.2", tag)
        self.run_script("sync")
        self.assertEqual(self.history.read_bytes(), expected)
        self.assertEqual(expected.count(b"## [0.4.0.1]"), 1)
        self.assertTrue(expected.endswith(first_history[first_history.index(b"## "):]))

    def test_product_ownership_is_independent(self):
        self.fragment(body="- OS entry.\n")
        other = self.root / "projects/start-sdk/changelog"
        other.mkdir(parents=True)
        other_fragment = other / "patch-fixed-example.md"
        other_fragment.write_text("- SDK entry.\n")
        self.commit()
        self.tag("0.4.0.1")
        self.git("tag", "start-sdk/v2.0.0")
        self.run_script("sync")
        os_history = self.history.read_bytes()
        sdk_history = (other.parent / "CHANGELOG.md").read_bytes()
        self.assertIn(b"OS entry", os_history)
        self.assertNotIn(b"SDK entry", os_history)
        self.assertIn(b"SDK entry", sdk_history)
        self.assertNotIn(b"OS entry", sdk_history)
        self.assertFalse(other_fragment.exists())

    def manifest(self, version, product=PRODUCT):
        name = Path(product).name
        if name == "start-os":
            path = self.root / "package.json"
        elif name == "start-sdk":
            path = self.root / product / "package.json"
        elif name == "start-wrt":
            path = self.root / product / "backend/ctrl/Cargo.toml"
        else:
            path = self.root / product / "Cargo.toml"
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps({"version": version}) if path.suffix == ".json"
                        else f'[package]\nversion = "{version}"\n')

    def test_version_reads_canonical_manifests_from_absolute_script(self):
        for name in ("start-os", "start-sdk", "start-cli", "start-tunnel", "start-registry", "start-wrt"):
            product = f"projects/{name}"
            version = "0.4.0.3" if name == "start-os" else "1.2.3"
            self.manifest(version, product)
            self.assertEqual(self.run_script("version", product).stdout.decode().strip(), version)
        self.run_script("version", "projects/unknown", success=False)

    def test_check_version_each_tier_and_mismatching_manifest(self):
        self.tag("0.4.0.3")
        for tier, expected in (("patch", "0.4.0.4"), ("minor", "0.4.1"), ("major", "0.5.0")):
            with self.subTest(tier=tier):
                fragment = self.fragment(f"{tier}-changed-impact.md")
                self.manifest(expected)
                self.run_script("check-version", PRODUCT)
                self.manifest("0.4.0.3")
                result = self.run_script("check-version", PRODUCT, success=False)
                self.assertIn(expected.encode(), result.stderr)
                fragment.unlink()

    def test_changed_unrelated_pr_after_ancestor_cli_release(self):
        product = "projects/start-cli"
        self.manifest("2.3.0", product)
        release_commit = self.commit()
        directory = self.root / product / "changelog"
        directory.mkdir()
        fragment = directory / "patch-fixed-pending.md"
        fragment.write_text("- Pending CLI fix.\n")
        base = self.commit()
        self.git("tag", "start-cli/v2.3.0", release_commit)
        (self.root / "README.md").write_text("Unrelated PR.\n")
        unrelated = self.commit()
        self.assertEqual(self.run_script("changed", base).stdout, b"\n")
        result = self.run_script("check-version", product, success=False)
        self.assertIn(b"expected '2.3.1'", result.stderr)
        fragment.write_text("- Revised pending CLI fix.\n")
        self.commit()
        self.assertEqual(self.run_script("changed", unrelated).stdout, b"projects/start-cli\n")
        result = self.run_script("check-version", product, success=False)
        self.assertIn(b"expected '2.3.1'", result.stderr)
        fragment_head = self.git("rev-parse", "HEAD")
        self.manifest("2.3.0+metadata", product)
        self.commit()
        self.assertEqual(self.run_script("changed", fragment_head).stdout, b"projects/start-cli\n")
        result = self.run_script("check-version", product, success=False)
        self.assertIn(b"expected '2.3.1'", result.stderr)

    def test_changed_uses_merge_base_not_base_tip(self):
        base = self.commit()
        self.git("checkout", "-qb", "base")
        self.manifest("1.2.3", "projects/start-sdk")
        base_tip = self.commit()
        self.git("checkout", "-q", "master")
        (self.root / "README.md").write_text("PR-only change.\n")
        self.commit()
        self.assertEqual(self.run_script("changed", base_tip).stdout, b"\n")
        self.fragment()
        self.commit()
        self.assertEqual(self.run_script("changed", base_tip).stdout, b"projects/start-os\n")
        self.assertEqual(self.run_script("changed", base).stdout, b"projects/start-os\n")

    def test_changed_deletion_rename_and_multiple_products(self):
        deleted = self.fragment("patch-fixed-deleted.md")
        renamed = self.fragment("patch-fixed-renamed.md")
        base = self.commit()
        deleted.unlink()
        destination = self.root / "projects/start-sdk/changelog/patch-fixed-renamed.md"
        destination.parent.mkdir(parents=True)
        renamed.rename(destination)
        self.commit()
        self.assertEqual(self.run_script("changed", base).stdout,
                         b"projects/start-os projects/start-sdk\n")
        deletion_base = self.git("rev-parse", "HEAD")
        destination.unlink()
        self.commit()
        self.assertEqual(self.run_script("changed", deletion_base).stdout, b"projects/start-sdk\n")
        self.manifest("1.2.3", "projects/start-cli")
        manifest_base = self.commit()
        (self.root / "projects/start-cli/Cargo.toml").rename(self.root / "removed-manifest.toml")
        self.commit()
        self.assertEqual(self.run_script("changed", manifest_base).stdout, b"projects/start-cli\n")

    def test_changed_selects_changelog_directory_replacements(self):
        base = self.git("rev-parse", "HEAD")
        self.fragments.rmdir()
        self.fragments.symlink_to(self.root, target_is_directory=True)
        self.commit()
        self.assertEqual(self.run_script("changed", base).stdout, b"projects/start-os\n")
        self.fragments.unlink()
        self.fragments.write_text("Not a directory.\n")
        replacement_base = self.git("rev-parse", "HEAD")
        self.commit()
        self.assertEqual(self.run_script("changed", replacement_base).stdout, b"projects/start-os\n")
        deletion_base = self.git("rev-parse", "HEAD")
        self.fragments.unlink()
        self.commit()
        self.assertEqual(self.run_script("changed", deletion_base).stdout, b"projects/start-os\n")

    def test_changed_matches_canonical_manifest_paths_exactly(self):
        for product in ("projects/start-os", "projects/start-sdk", "projects/start-cli",
                        "projects/start-tunnel", "projects/start-registry", "projects/start-wrt"):
            with self.subTest(product=product):
                base = self.git("rev-parse", "HEAD")
                self.manifest("0.4.0.3" if product == PRODUCT else "1.2.3", product)
                self.commit()
                self.assertEqual(self.run_script("changed", base).stdout, f"{product}\n".encode())
        base = self.git("rev-parse", "HEAD")
        for name in ("projects/start-os/Cargo.toml", "projects/start-os/package.json",
                     "projects/start-wrt/Cargo.toml", "projects/start-sdk/package.json.backup",
                     "projects/start-cli/changelog-backup/patch-fixed-example.md"):
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_text("Unrelated path.\n")
        self.commit()
        self.assertEqual(self.run_script("changed", base).stdout, b"\n")

    def test_changed_handles_nul_delimited_paths(self):
        base = self.git("rev-parse", "HEAD")
        self.fragment("patch-fixed-name\nwith-newline.md")
        self.commit()
        self.assertEqual(self.run_script("changed", base).stdout, b"projects/start-os\n")
        self.run_script("changed", "nonexistent-base", success=False)

    def test_projects_preserves_release_caller_contract(self):
        self.assertEqual(self.run_script("projects").stdout,
                         b"start-os start-sdk start-cli start-tunnel start-registry start-wrt\n")

    def test_mixed_tiers_use_highest_and_omit_startos_zero_revision(self):
        self.tag("0.4.0.3")
        self.fragment("patch-fixed-small.md")
        self.fragment("minor-added-medium.md")
        self.manifest("0.4.1")
        self.run_script("check-version", PRODUCT)
        self.run_script("check-version", PRODUCT, "0.4.1.0", success=False)
        self.fragment("major-removed-large.md")
        self.manifest("0.5.0")
        self.run_script("check-version", PRODUCT)
        self.run_script("check-version", PRODUCT, "0.4.1", success=False)

    def test_check_version_validates_malformed_fragments_even_without_baseline(self):
        self.manifest("0.4.0.3")
        for name, body in (("fixed-invalid.md", "- Invalid name.\n"),
                           ("patch-fixed-invalid.md", "## Invalid heading\n")):
            with self.subTest(name=name):
                fragment = self.fragment(name, body)
                self.run_script("check-version", PRODUCT, success=False)
                fragment.unlink()
        self.fragment("major-added-first-release.md")
        self.run_script("check-version", PRODUCT)
        self.run_script("check-version", PRODUCT, "0.9.0")
        self.run_script("check-version", PRODUCT, "1.0.0", success=False)

    def test_no_fragments_keeps_latest_release_and_override_is_caller_data(self):
        self.tag("0.4.0.3")
        self.manifest("0.4.0.3")
        self.run_script("check-version", PRODUCT)
        self.manifest("0.4.0.4")
        self.run_script("check-version", PRODUCT, success=False)
        self.run_script("check-version", PRODUCT, "0.4.0.3")
        (self.root / "package.json").unlink()
        self.run_script("check-version", PRODUCT, "0.4.0.3")
        self.run_script("check-version", PRODUCT, success=False)

    def test_lingering_released_tiers_are_not_counted_again(self):
        self.fragment("major-added-released.md", "- Already released.\n")
        self.commit()
        self.tag("0.4.0.3", annotated=True)
        self.manifest("0.4.0.3")
        self.run_script("check-version", PRODUCT)
        self.fragment("patch-fixed-pending.md")
        self.manifest("0.4.0.4")
        self.run_script("check-version", PRODUCT)
        self.run_script("check-version", PRODUCT, "0.5.0", success=False)

    def test_modified_path_is_pending_but_identical_ancestor_path_is_not(self):
        path = self.fragment("minor-added-feature.md", "- Released feature.\n")
        self.commit()
        self.tag("0.4.0.3")
        path.write_text("- Revised feature.\n")
        self.manifest("0.4.1")
        self.run_script("check-version", PRODUCT)
        self.run_script("check-version", PRODUCT, "0.4.0.3", success=False)

    def test_live_origin_baseline_is_fresh_and_local_unpublished_tags_ignored(self):
        remote = self.root / "remote.git"
        self.git("init", "--bare", "-q", str(remote))
        self.git("remote", "set-url", "origin", str(remote))
        self.tag("0.4.0.3")
        self.git("push", "-q", "origin", "start-os/v0.4.0.3")
        self.fragment()
        self.manifest("0.4.0.4")
        self.tag("0.9.0")
        self.run_script("check-version", PRODUCT)
        self.tag("0.4.0.8")
        self.git("push", "-q", "origin", "start-os/v0.4.0.8")
        self.run_script("check-version", PRODUCT, success=False)
        self.manifest("0.4.0.9")
        self.run_script("check-version", PRODUCT)

    def test_latest_stable_is_numeric_not_topological_and_prereleases_ignored(self):
        self.tag("0.4.0.10")
        self.commit()
        self.tag("0.4.0.9")
        self.tag("0.5.0-rc.1")
        self.fragment()
        self.manifest("0.4.0.11")
        self.run_script("check-version", PRODUCT)

    def test_check_version_queries_origin_once(self):
        self.tag("0.4.0.3")
        self.manifest("0.4.0.3")
        trace = self.root / "git.trace"
        self.env["GIT_TRACE"] = str(trace)
        self.run_script("check-version", PRODUCT)
        self.assertEqual(trace.read_text().count("built-in: git ls-remote --tags origin"), 1)

    def test_unknown_origin_tag_namespace_is_ignored(self):
        self.git("tag", "unrelated/vnot-a-version")
        self.tag("0.4.0.3")
        self.manifest("0.4.0.3")
        self.run_script("check-version", PRODUCT)
        self.run_script("sync")

    def test_nonancestor_release_is_baseline_but_does_not_own_pending_fragments(self):
        self.git("checkout", "-qb", "unmerged")
        self.fragment()
        self.commit()
        self.tag("0.4.0.3")
        self.git("checkout", "-q", "master")
        self.fragment()
        self.manifest("0.4.0.4")
        self.run_script("check-version", PRODUCT)

    def test_check_version_semver_product(self):
        product = "projects/start-sdk"
        directory = self.root / product / "changelog"
        directory.mkdir(parents=True)
        self.git("tag", "start-sdk/v2.0.9")
        fragment = directory / "minor-added-feature.md"
        fragment.write_text("- Feature.\n")
        self.manifest("2.1.0", product)
        self.run_script("check-version", product)
        self.run_script("check-version", product, "2.0.10", success=False)

    def test_consumption_does_not_follow_replacement_symlink(self):
        fragment = self.fragment()
        self.commit()
        self.tag("0.4.0.1")
        outside = self.root / "outside.md"
        outside.write_bytes(fragment.read_bytes())
        fragment.unlink()
        fragment.symlink_to(outside)
        self.run_script("sync")
        self.assertTrue(fragment.is_symlink())
        self.assertTrue(outside.exists())


if __name__ == "__main__":
    unittest.main()
