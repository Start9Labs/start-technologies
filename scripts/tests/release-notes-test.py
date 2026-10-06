#!/usr/bin/env python3
import os
from pathlib import Path
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
        self.notes = self.root / "projects/start-sdk/release-notes"
        self.notes.mkdir(parents=True)
        self.main = self.notes / f"{VERSION}.md"
        self.main.write_text(MAIN)
        self.pre_update = self.notes / f"{VERSION}.pre-update.md"
        self.env = dict(os.environ, VERSION=VERSION, CHANGELOG_REF="fixture-ref")

    def run_script(self, command="notes", project="start-sdk", success=True):
        result = subprocess.run(
            ["bash", str(self.root / "scripts/manage-release.sh"), command, project],
            cwd=self.root, env=self.env, capture_output=True, text=True,
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
        self.assertIn("/blob/fixture-ref/", combined)

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
        command = '''source <(awk '/^# --- Dispatch ---/{exit} {print}' "$1")
REPO_ROOT="$2"; PROJECT=start-sdk; VERSION=1.2.3; COMMIT="$3"
assert_metadata_matches_adopted'''

        def assert_blocked():
            result = subprocess.run(["bash", "-c", command, "test", str(script), str(self.root), adopted],
                                    capture_output=True, text=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("1.2.3.pre-update.md differs", result.stderr)

        self.pre_update.write_text(PRE_UPDATE)
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
