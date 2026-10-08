#!/usr/bin/env python3
import os
from pathlib import Path
import re
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
VERSION = "1.2.3"
MAIN = "Lede.\n\n## Highlights\n\n- Feature.\n\n## Important\n\nFollow up.\n"
PRE_UPDATE = "## ⚠️ Before You Update\n\n> Prepare first."


class ReleaseNotesTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "scripts").mkdir()
        shutil.copy2(ROOT / "scripts/manage-release.sh", self.root / "scripts")
        for script in ("changelog.mjs", "changelog-version.mjs"):
            shutil.copy2(ROOT / f"scripts/{script}", self.root / "scripts")
        for config in ("package.json", ".prettierrc.json"):
            shutil.copy2(ROOT / config, self.root / config)
        self.notes = self.root / "projects/start-sdk/release-notes"
        self.notes.mkdir(parents=True)
        self.main = self.notes / f"{VERSION}.md"
        self.main.write_text(MAIN)
        self.pre_update = self.notes / f"{VERSION}.pre-update.md"
        self.env = dict(os.environ, VERSION=VERSION, CHANGELOG_REF="fixture-ref")

    def run_script(self, command="notes", project="start-sdk", success=True, root=None):
        root = root or self.root
        result = subprocess.run(
            ["bash", str(root / "scripts/manage-release.sh"), command, project],
            cwd=root, env=self.env, capture_output=True, text=True,
        )
        if success:
            self.assertEqual(result.returncode, 0, result.stderr)
        else:
            self.assertNotEqual(result.returncode, 0)
        return result

    def test_optional_companion(self):
        without = self.run_script().stdout
        self.pre_update.write_text("")
        self.assertEqual(self.run_script().stdout, without)
        self.pre_update.write_text(PRE_UPDATE)
        combined = self.run_script().stdout
        self.assertEqual(combined, PRE_UPDATE + "\n\n" + without)
        self.assertLess(combined.index("Full changelog"), combined.index("## Important"))
        self.assertEqual(combined.count("Full changelog"), 1)
        self.assertIn("/blob/fixture-ref/projects/start-sdk/CHANGELOG.md#123", combined)

    def test_reads_canonical_version_without_override(self):
        del self.env["VERSION"]
        (self.root / "projects/start-sdk/package.json").write_text('{"version":"1.2.3"}')
        self.assertIn("v1.2.3", self.run_script().stdout)

    def block_python(self):
        bin_dir = self.root / "bin"
        bin_dir.mkdir()
        marker = self.root / "python-called"
        for name in ("python", "python3"):
            executable = bin_dir / name
            executable.write_text(f'#!/bin/sh\ntouch "{marker}"\nexit 97\n')
            executable.chmod(0o755)
        self.env["PATH"] = f"{bin_dir}:{os.environ['PATH']}"
        return marker

    def test_canonical_versions_and_notes_do_not_require_python(self):
        del self.env["VERSION"]
        marker = self.block_python()
        manifests = (
            ("start-sdk", "projects/start-sdk/package.json", '{"version":"1.2.3"}', VERSION, "123"),
            ("start-os", "package.json", '{"version":"0.4.0.3"}', "0.4.0.3", "0403"),
            ("start-cli", "projects/start-cli/Cargo.toml", '[package]\nversion = "1.2.3"\n', VERSION, "123"),
        )
        for project, manifest, contents, version, anchor in manifests:
            with self.subTest(project=project):
                path = self.root / manifest
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_text(contents)
                notes = self.root / "projects" / project / "release-notes" / f"{version}.md"
                notes.parent.mkdir(parents=True, exist_ok=True)
                notes.write_text(MAIN)
                output = self.run_script(project=project).stdout
                self.assertIn("- Feature.", output)
                self.assertIn(f"/projects/{project}/CHANGELOG.md#{anchor}", output)
        self.assertFalse(marker.exists(), "release caller invoked Python")

    def test_sparse_deploy_canonical_versions_and_notes(self):
        del self.env["VERSION"]
        marker = self.block_python()
        (self.root / ".prettierrc.json").unlink()
        (self.root / "package.json").write_text('{"version":"0.4.0.3"}')
        manifest = self.root / "projects/start-wrt/backend/ctrl/Cargo.toml"
        manifest.parent.mkdir(parents=True)
        manifest.write_text('[package]\nname = "ctrl"\nversion = "1.2.3"\n')
        cases = (("start-os", "startos-iso.yaml", "0.4.0.3", "0403"),
                 ("start-wrt", "start-wrt.yaml", VERSION, "123"))
        for project, _, version, _ in cases:
            notes = self.root / "projects" / project / "release-notes"
            notes.mkdir(parents=True)
            (notes / f"{version}.md").write_text(MAIN)
            (notes / f"{version}.pre-update.md").write_text(PRE_UPDATE)
        git = ["git", "-C", str(self.root)]
        subprocess.run(git + ["init", "-q"], check=True)
        subprocess.run(git + ["add", "scripts", "projects", "package.json"], check=True)
        subprocess.run(git + ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.com",
                              "-c", "commit.gpgsign=false", "commit", "-qm", "fixture"], check=True)
        ref = subprocess.check_output(git + ["rev-parse", "HEAD"], text=True).strip()
        self.env["CHANGELOG_REF"] = ref
        for project, workflow, version, anchor in cases:
            with self.subTest(project=project), tempfile.TemporaryDirectory() as directory:
                deploy = (ROOT / ".github/workflows" / workflow).read_text().split("\n  deploy:\n", 1)[1]
                sparse = re.search(r"          sparse-checkout: \|\n((?:            .+\n)+)", deploy)
                self.assertIsNotNone(sparse)
                patterns = "\n".join(line.strip() for line in sparse[1].splitlines()) + "\n"
                checkout = Path(directory) / "checkout"
                subprocess.run(["git", "clone", "-q", "--no-checkout", str(self.root), str(checkout)], check=True)
                git = ["git", "-C", str(checkout)]
                mode = "--no-cone" if "sparse-checkout-cone-mode: false" in deploy else "--cone"
                subprocess.run(git + ["sparse-checkout", "init", mode], check=True)
                subprocess.run(git + ["sparse-checkout", "set", "--stdin"], input=patterns, text=True, check=True)
                subprocess.run(git + ["checkout", "-q"], check=True)
                self.assertFalse((checkout / ".prettierrc.json").exists())
                self.assertFalse((checkout / "node_modules").exists())
                if project == "start-wrt":
                    self.assertFalse((checkout / "package.json").exists())
                determine = re.search(r"      - name: Determine version\n        id: version\n        run: \|\n((?:          .+\n)+)", deploy)
                self.assertIsNotNone(determine)
                output_file = Path(directory) / "outputs"
                result = subprocess.run(["bash", "-e", "-c", determine[1]], cwd=checkout,
                                        env=dict(self.env, GITHUB_OUTPUT=str(output_file)),
                                        capture_output=True, text=True)
                self.assertEqual(result.returncode, 0, result.stderr)
                self.assertEqual(output_file.read_text(), f"version={version}\n")
                derived = self.run_script(project=project, root=checkout).stdout
                self.assertIn(PRE_UPDATE, derived)
                self.assertIn("- Feature.", derived)
                self.assertIn(f"/blob/{ref}/projects/{project}/CHANGELOG.md#{anchor}", derived)
                self.env["VERSION"] = version
                canonical_manifest = checkout / ("package.json" if project == "start-os" else
                                                  "projects/start-wrt/backend/ctrl/Cargo.toml")
                canonical_manifest.unlink()
                self.assertEqual(self.run_script(project=project, root=checkout).stdout, derived)
                del self.env["VERSION"]
        self.assertFalse(marker.exists(), "deploy release caller invoked Python")

    def test_release_links_rendered_changelog(self):
        del self.env["CHANGELOG_REF"]
        for project, version, anchor in (("start-sdk", "1.2.3", "123"),
                                         ("start-os", "0.4.0.3", "0403"),
                                         ("start-os", "0.4.1", "041"),
                                         ("start-sdk", "1.2.3-RC.1+Build.2", "123-rc1build2")):
            with self.subTest(project=project, version=version):
                self.env["VERSION"] = version
                notes = self.root / "projects" / project / "release-notes" / f"{version}.md"
                notes.parent.mkdir(parents=True, exist_ok=True)
                notes.write_text(MAIN)
                output = self.run_script(project=project).stdout
                self.assertIn(f"/blob/master/projects/{project}/CHANGELOG.md#{anchor}", output)
                self.assertNotIn("/releases/download/", output)

    def test_source_links_survive_fragment_consumption(self):
        git = ["git", "-C", str(self.root)]
        subprocess.run(git + ["init", "-q"], check=True)
        fragments = self.root / "projects/start-sdk/changelog"
        fragments.mkdir()
        fragment = fragments / "patch-fixed-example.md"
        fragment.write_text("- Fixed feature.\n")
        history = self.root / "projects/start-sdk/CHANGELOG.md"
        history.write_text("# Changelog\n\n## [1.2.2]\n\n- Previous release.\n")
        subprocess.run(git + ["add", "."], check=True)
        commit = git + ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.com",
                        "-c", "commit.gpgsign=false", "commit", "-qm", "fixture"]
        subprocess.run(commit, check=True)
        self.env["CHANGELOG_REF"] = "HEAD"
        self.assertIn("/tree/HEAD/projects/start-sdk/changelog", self.run_script().stdout)
        subprocess.run(git + ["rm", str(fragment)], check=True)
        history.write_text("# Changelog\n\n## [1.2.3]\n\n- Fixed feature.\n")
        subprocess.run(git + ["add", str(history)], check=True)
        subprocess.run(commit, check=True)
        output = self.run_script().stdout
        self.assertIn("/blob/HEAD/projects/start-sdk/CHANGELOG.md#123", output)
        self.assertNotIn("/tree/", output)

    def test_github_and_registry_share_composition(self):
        self.pre_update.write_text(PRE_UPDATE)
        notes = self.run_script().stdout
        body = self.run_script("body").stdout
        self.assertTrue(body.endswith("\n" + notes + "\n"))
        self.assertEqual(body.count(PRE_UPDATE), 1)

    def test_missing_main_fails_even_with_companion(self):
        self.pre_update.write_text(PRE_UPDATE)
        self.main.unlink()
        self.assertIn("No release notes", self.run_script(success=False).stderr)
        self.assertIn("No release notes", self.run_script("body", success=False).stderr)

    def test_existing_changelog_link_is_replaced(self):
        self.main.write_text(MAIN.replace("- Feature.", "- Feature.\n\n**[Full changelog old](old)\n"))
        self.pre_update.write_text(PRE_UPDATE)
        notes = self.run_script().stdout
        self.assertNotIn("](old)", notes)
        self.assertEqual(notes.count("Full changelog"), 1)

    def test_companion_changes_block_adopted_release(self):
        subprocess.run(["git", "init", "-q", self.root], check=True)
        git = ["git", "-C", str(self.root)]

        def commit():
            subprocess.run(git + ["add", "."], check=True)
            subprocess.run(git + ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.com",
                                  "-c", "commit.gpgsign=false", "commit", "-qm", "fixture"], check=True)

        commit()
        adopted = subprocess.check_output(git + ["rev-parse", "HEAD"], text=True).strip()
        script = self.root / "scripts/manage-release.sh"
        command = '''source <(awk '/^SUBCOMMAND=/{exit} {print}' "$1")
REPO_ROOT="$2"; PROJECT=start-sdk; VERSION=1.2.3; COMMIT="$3"
assert_metadata_matches_adopted'''

        def assert_blocked(name="1.2.3.pre-update.md"):
            result = subprocess.run(["bash", "-c", command, "test", str(script), str(self.root), adopted],
                                    capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn(f"{name} differs", result.stderr)

        self.pre_update.write_text(PRE_UPDATE)
        assert_blocked()
        subprocess.run(git + ["add", str(self.pre_update)], check=True)
        assert_blocked()
        self.main.write_text(MAIN + "Uncommitted instruction.\n")
        assert_blocked("1.2.3.md")
        self.main.write_text(MAIN)
        commit()
        assert_blocked()
        adopted = subprocess.check_output(git + ["rev-parse", "HEAD"], text=True).strip()
        self.pre_update.write_text(PRE_UPDATE + "\nChanged warning.\n")
        commit()
        assert_blocked()
        adopted = subprocess.check_output(git + ["rev-parse", "HEAD"], text=True).strip()
        self.pre_update.unlink()
        commit()
        assert_blocked()
        adopted = subprocess.check_output(git + ["rev-parse", "HEAD"], text=True).strip()
        history = self.root / "projects/start-sdk/CHANGELOG.md"
        history.write_text("# Generated history\n")
        commit()
        result = subprocess.run(["bash", "-c", command, "test", str(script), str(self.root), adopted],
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stderr)
        fragments = self.root / "projects/start-sdk/changelog"
        fragments.mkdir()
        fragment = fragments / "patch-fixed-example.md"
        fragment.write_text("- Fixed a released feature.\n")
        commit()
        result = subprocess.run(["bash", "-c", command, "test", str(script), str(self.root), adopted],
                                capture_output=True, text=True)
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("changelog differs", result.stderr)

    def test_release_uploads_changelog_from_tag_before_publication(self):
        subprocess.run(["git", "init", "-q", self.root], check=True)
        product = self.root / "projects/start-sdk"
        (product / "CHANGELOG.md").write_text("# Changelog\n\n## [1.2.2]\n\n- Old change.\n")
        fragments = product / "changelog"
        fragments.mkdir()
        (fragments / "patch-fixed-example.md").write_text("- Fixed the released feature.\n")
        git = ["git", "-C", str(self.root)]
        subprocess.run(git + ["add", "."], check=True)
        subprocess.run(git + ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.com",
                              "-c", "commit.gpgsign=false", "commit", "-qm", "fixture"], check=True)
        subprocess.run(git + ["-c", "tag.gpgsign=false", "tag", "start-sdk/v1.2.3"], check=True)
        remote = self.root / "origin.git"
        subprocess.run(["git", "init", "--bare", "-q", remote], check=True)
        subprocess.run(git + ["remote", "add", "origin", str(remote)], check=True)
        subprocess.run(git + ["push", "-q", "origin", "HEAD:master", "--tags"], check=True)
        (fragments / "minor-added-later.md").write_text("- Later change.\n")
        subprocess.run(git + ["add", "projects"], check=True)
        subprocess.run(git + ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.com",
                              "-c", "commit.gpgsign=false", "commit", "-qm", "later"], check=True)
        bin_dir = self.root / "bin"
        bin_dir.mkdir()
        gh = bin_dir / "gh"
        captured = self.root / "uploaded.md"
        gh.write_text('''#!/usr/bin/env python3
import os, pathlib, sys
args = sys.argv[1:]
with open(os.environ["CALLS"], "a") as calls:
    calls.write(" ".join(args) + "\\n")
if args[:2] == ["release", "view"]:
    sys.exit(0 if os.environ.get("EXISTING_RELEASE") else 1)
if args[:2] == ["release", "upload"]:
    source = next(pathlib.Path(arg) for arg in args if arg.endswith("/CHANGELOG.md"))
    pathlib.Path(os.environ["CAPTURE"]).write_text(source.read_text())
''')
        gh.chmod(0o755)
        calls = self.root / "gh-calls"
        self.env.update(PATH=f"{bin_dir}:{os.environ['PATH']}", CAPTURE=str(captured), CALLS=str(calls))
        self.run_script("create-gh-release")
        self.assertIn("release create", calls.read_text())
        calls.unlink()
        self.env["EXISTING_RELEASE"] = "1"
        self.run_script("create-gh-release")
        self.assertIn("release edit", calls.read_text())
        text = captured.read_text()
        self.assertIn("## [1.2.3]", text)
        self.assertIn("Fixed the released feature.", text)
        self.assertIn("Old change.", text)
        self.assertNotIn("Later change.", text)
        (fragments / "patch-fixed-example.md").write_text("invalid fragment\n")
        subprocess.run(git + ["add", "projects"], check=True)
        subprocess.run(git + ["-c", "user.name=Fixture", "-c", "user.email=fixture@example.com",
                              "-c", "commit.gpgsign=false", "commit", "-qm", "invalid"], check=True)
        subprocess.run(git + ["-c", "tag.gpgsign=false", "tag", "-f", "start-sdk/v1.2.3"], check=True)
        subprocess.run(git + ["push", "-q", "--force", "origin", "refs/tags/start-sdk/v1.2.3"], check=True)
        captured.unlink()
        for existing in ("", "1"):
            with self.subTest(existing_release=bool(existing)):
                self.env["EXISTING_RELEASE"] = existing
                if calls.exists():
                    calls.unlink()
                self.run_script("create-gh-release", success=False)
                self.assertFalse(captured.exists())
                self.assertFalse(calls.exists())

    def test_startos_registry_and_packaged_welcome(self):
        version = "0.4.0.2"
        source = ROOT / "projects/start-os/release-notes"
        target = self.root / "projects/start-os/release-notes"
        target.mkdir(parents=True)
        for name in (f"{version}.md", f"{version}.pre-update.md"):
            shutil.copy2(source / name, target / name)
        self.env["VERSION"] = version
        combined = self.run_script(project="start-os").stdout
        self.assertIn("## ⚠️ Before You Update", combined)
        self.assertIn("only way to update from 0.3.5.1", combined)
        self.assertIn((source / f"{version}.pre-update.md").read_text().strip(), combined)
        stage = self.root / "image/usr/lib/startos"
        stage.mkdir(parents=True)
        version_file = self.root / "VERSION.txt"
        version_file.write_text(version)
        harness = self.root / "install.mk"
        harness.write_text(f'''mkdir = true
rm = true
ln = true
cp = $(if $(filter %/release-notes.md,$(2)),cp "$(1)" "$(2)",true)
include {ROOT}/projects/start-os/build.mk
''')
        result = subprocess.run(
            ["make", "-f", str(harness), "start-os-install", "STARTOS_TARGETS=", "PLATFORM=x86_64",
             f"VERSION_FILE={version_file}", f"DESTDIR={self.root}/image", "ENVIRONMENT="],
            cwd=ROOT, capture_output=True, text=True,
        )
        self.assertEqual(result.returncode, 0, result.stderr)
        packaged = (stage / "release-notes.md").read_text()
        self.assertEqual(packaged, (source / f"{version}.md").read_text())
        self.assertNotIn("Before You Update", packaged)
        self.assertNotIn("only way to update", packaged)
        self.assertIn("## Highlights", packaged)


if __name__ == "__main__":
    unittest.main()
