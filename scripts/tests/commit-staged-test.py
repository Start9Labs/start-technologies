#!/usr/bin/env python3
import base64
import json
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "commit-staged.mjs"


class CommitStagedTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.env = dict(os.environ, GIT_CONFIG_COUNT="0", GIT_CONFIG_NOSYSTEM="1")
        self.git("init", "-q")
        self.git("config", "user.name", "Fixture")
        self.git("config", "user.email", "fixture@example.com")
        self.git("config", "commit.gpgsign", "false")
        (self.root / "old.md").write_text("old\n")
        self.git("add", ".")
        self.git("commit", "-qm", "fixture")
        self.base = self.git("rev-parse", "HEAD").strip()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        gh = self.bin / "gh"
        gh.write_text('''#!/usr/bin/env python3
import json, os, pathlib, sys
pathlib.Path(os.environ["CAPTURE"]).write_text(sys.stdin.read())
if os.environ.get("FAIL"):
    print('{"errors": [{"message": "head moved"}]}')
else:
    print('{"data":{"createCommitOnBranch":{"commit":{"oid":"signed-oid"}}}}')
''')
        gh.chmod(0o755)
        self.capture = self.root / "payload.json"
        self.message = self.root / "message"
        self.message.write_text("chore: archive fragments\n\nDetails.\n")
        self.env.update(PATH=f"{self.bin}:{os.environ['PATH']}",
                        GITHUB_REPOSITORY="Start9Labs/fixture", CAPTURE=str(self.capture))

    def git(self, *args):
        return subprocess.check_output(["git", *args], cwd=self.root, env=self.env, text=True)

    def run_script(self):
        return subprocess.run(["node", str(SCRIPT), str(self.message), "master", self.base],
                              cwd=self.root, env=self.env, capture_output=True, text=True)

    def test_rename_large_content_and_index_ownership(self):
        self.git("mv", "old.md", "name with spaces.md")
        contents = "Large prose.\n" * 20000
        (self.root / "name with spaces.md").write_text(contents)
        self.git("add", "name with spaces.md")
        (self.root / "name with spaces.md").write_text("unstaged content")
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        payload = json.loads(self.capture.read_text())["variables"]["input"]
        self.assertEqual(payload["expectedHeadOid"], self.base)
        self.assertEqual(payload["branch"], {"repositoryNameWithOwner": "Start9Labs/fixture", "branchName": "master"})
        self.assertEqual(payload["message"], {"headline": "chore: archive fragments", "body": "Details."})
        self.assertEqual(payload["fileChanges"]["deletions"], [{"path": "old.md"}])
        addition, = payload["fileChanges"]["additions"]
        self.assertEqual(addition["path"], "name with spaces.md")
        self.assertEqual(base64.b64decode(addition["contents"]).decode(), contents)

    def test_staged_binary_blob_over_one_mib(self):
        contents = bytes(range(256)) * 8192
        (self.root / "large.bin").write_bytes(contents)
        self.git("add", "large.bin")
        (self.root / "large.bin").write_bytes(b"unstaged")
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(result.stdout, "signed-oid\n")
        additions = json.loads(self.capture.read_text())["variables"]["input"]["fileChanges"]["additions"]
        self.assertEqual(additions[0]["path"], "large.bin")
        self.assertEqual(base64.b64decode(additions[0]["contents"]), contents)

    def test_modes_are_not_silently_rewritten(self):
        for mode in ("symlink", "executable"):
            with self.subTest(mode=mode):
                self.git("reset", "--hard", self.base)
                path = self.root / "unsupported"
                if path.exists() or path.is_symlink():
                    path.unlink()
                if mode == "symlink":
                    path.symlink_to("old.md")
                else:
                    path.write_text("#!/bin/sh\n")
                    path.chmod(0o755)
                self.git("add", "unsupported")
                self.assertEqual(self.run_script().returncode, 2)
                self.assertFalse(self.capture.exists())

    def test_deletion_only(self):
        self.git("rm", "old.md")
        result = self.run_script()
        self.assertEqual(result.returncode, 0, result.stderr)
        changes = json.loads(self.capture.read_text())["variables"]["input"]["fileChanges"]
        self.assertEqual(changes, {"additions": [], "deletions": [{"path": "old.md"}]})

    def test_existing_unsupported_modes_cannot_be_deleted_or_normalized(self):
        for mode in ("symlink", "executable", "gitlink"):
            for change in ("delete", "normalize"):
                with self.subTest(mode=mode, change=change):
                    self.git("reset", "--hard", self.base)
                    path = self.root / "unsupported"
                    if path.exists() or path.is_symlink():
                        path.unlink()
                    if mode == "gitlink":
                        self.git("update-index", "--add", "--cacheinfo", f"160000,{self.base},unsupported")
                    else:
                        if mode == "symlink":
                            path.symlink_to("old.md")
                        else:
                            path.write_text("#!/bin/sh\n")
                            path.chmod(0o755)
                        self.git("add", "unsupported")
                    self.git("commit", "-qm", "unsupported mode")
                    old_base = self.base
                    self.base = self.git("rev-parse", "HEAD").strip()
                    if path.exists() or path.is_symlink():
                        path.unlink()
                    self.git("update-index", "--force-remove", "unsupported")
                    if change == "normalize":
                        path.write_text("ordinary file\n")
                        self.git("add", "unsupported")
                    result = self.run_script()
                    self.assertEqual(result.returncode, 2, result.stderr)
                    self.assertFalse(self.capture.exists())
                    self.base = old_base

    def test_new_gitlink_is_rejected(self):
        self.git("update-index", "--add", "--cacheinfo", f"160000,{self.base},submodule")
        result = self.run_script()
        self.assertEqual(result.returncode, 2, result.stderr)
        self.assertFalse(self.capture.exists())

    def test_graphql_errors_fail_even_with_http_success(self):
        (self.root / "old.md").write_text("changed\n")
        self.git("add", "old.md")
        self.env["FAIL"] = "1"
        result = self.run_script()
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("head moved", result.stderr)

    def test_tag_compiler_workflow_with_local_commit_api(self):
        repo_root = SCRIPT.parents[1]
        scripts = self.root / "scripts"
        scripts.mkdir()
        for name in ("changelog.mjs", "changelog-version.mjs", "commit-staged.mjs"):
            shutil.copy2(repo_root / "scripts" / name, scripts / name)
        shutil.copy2(repo_root / ".prettierrc.json", self.root / ".prettierrc.json")
        pin = json.loads((repo_root / "package.json").read_text())["devDependencies"]["prettier"]
        (self.root / "package.json").write_text(json.dumps({"devDependencies": {"prettier": pin}}))
        product = self.root / "projects/start-sdk"
        fragments = product / "changelog"
        fragments.mkdir(parents=True)
        fragment = fragments / "patch-fixed-released.md"
        fragment.write_text("- Released fix.\n\n- Second released fix.\n")
        (fragments / "patch-fixed-second-released.md").write_text("- Third released fix.\n")
        (product / "CHANGELOG.md").write_text("# Changelog\n")
        self.git("add", "scripts", "projects", "package.json", ".prettierrc.json")
        self.git("commit", "-qm", "release")
        self.git("-c", "tag.gpgsign=false", "tag", "start-sdk/v1.2.3")
        remote = self.root / "origin.git"
        subprocess.run(["git", "init", "--bare", "-q", remote], check=True)
        self.git("remote", "add", "origin", str(remote))
        self.git("push", "-q", "origin", "HEAD:master", "HEAD:live-docs", "--tags")
        self.git("fetch", "-q", "origin")
        self.git("checkout", "--detach", "origin/live-docs")
        (self.bin / "gh").write_text('''#!/usr/bin/env python3
import base64, json, os, pathlib, subprocess, sys, tempfile
payload = json.load(sys.stdin)["variables"]["input"]
remote = os.environ["LOCAL_REMOTE"]
env = dict(os.environ)
marker = pathlib.Path(os.environ["RUNNER_TEMP"]) / "raced"
race = not marker.exists()
if race:
    marker.touch()
    payload["fileChanges"] = {"deletions": [], "additions": [{
        "path": "projects/start-sdk/changelog/patch-fixed-after-tag.md",
        "contents": base64.b64encode(b"- New work after the tag.\\n").decode(),
    }]}
def git(*args, data=None):
    return subprocess.check_output(["git", "--git-dir", remote, *args], input=data, env=env).decode().strip()
with tempfile.TemporaryDirectory() as temp:
    env["GIT_INDEX_FILE"] = temp + "/index"
    env["GIT_WORK_TREE"] = temp
    git("read-tree", payload["expectedHeadOid"])
    for deletion in payload["fileChanges"]["deletions"]:
        git("update-index", "--force-remove", deletion["path"])
    for addition in payload["fileChanges"]["additions"]:
        blob = git("hash-object", "-w", "--stdin", data=base64.b64decode(addition["contents"]))
        git("update-index", "--add", "--cacheinfo", "100644," + blob + "," + addition["path"])
    tree = git("write-tree")
    oid = git("-c", "user.name=Fixture", "-c", "user.email=fixture@example.com",
              "-c", "commit.gpgsign=false", "commit-tree", tree, "-p", payload["expectedHeadOid"],
              data=payload["message"]["headline"].encode())
    git("update-ref", "refs/heads/master", oid, payload["expectedHeadOid"])
if race:
    print('{"errors": [{"message": "head moved"}]}')
else:
    print(json.dumps({"data": {"createCommitOnBranch": {"commit": {"oid": oid}}}}))
''')
        workflow = (repo_root / ".github/workflows/docs-sync-on-tag.yml").read_text()
        block = workflow.split("      - name: Compile released changelogs onto master\n", 1)[1]
        block = block.split("        run: |\n", 1)[1].split("\n      - name:", 1)[0]
        shell = "\n".join(line[10:] for line in block.splitlines())
        runner_temp = self.root / "runner-temp"
        runner_temp.mkdir()
        self.env.update(LOCAL_REMOTE=str(remote), RUNNER_TEMP=str(runner_temp),
                        GITHUB_REF_NAME="start-sdk/v1.2.3")
        result = subprocess.run(["bash", "-c", shell], cwd=self.root, env=self.env,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.git("status", "--porcelain", "--untracked-files=no"), "")
        self.assertEqual(self.git("rev-parse", "HEAD"), self.git("rev-parse", "origin/live-docs"))
        history = self.git("show", "origin/master:projects/start-sdk/CHANGELOG.md")
        self.assertEqual(history, "# Changelog\n\n## [1.2.3]\n\n### Fixed\n\n"
                                 "- Released fix.\n\n- Second released fix.\n\n- Third released fix.\n")
        self.assertNotIn("patch-fixed-released.md", self.git("ls-tree", "-r", "--name-only", "origin/master", "projects"))
        self.assertIn("New work after the tag.", self.git("show", "origin/master:projects/start-sdk/changelog/patch-fixed-after-tag.md"))
        self.assertNotIn("New work after the tag.", history)
        self.assertIn("head moved", result.stderr)
        master = self.git("rev-parse", "origin/master")
        result = subprocess.run(["bash", "-c", shell], cwd=self.root, env=self.env,
                                capture_output=True, text=True)
        self.assertEqual(result.returncode, 0, result.stdout + result.stderr)
        self.assertEqual(self.git("rev-parse", "origin/master"), master)
        self.assertEqual(self.git("show", "origin/master:projects/start-sdk/CHANGELOG.md"), history)

    def test_empty_index_makes_no_request(self):
        self.assertEqual(self.run_script().returncode, 0)
        self.assertFalse(self.capture.exists())


if __name__ == "__main__":
    unittest.main()
