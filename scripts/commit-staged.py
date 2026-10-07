#!/usr/bin/env python3
import base64
import json
import os
from pathlib import Path
import subprocess
import sys


def git(*args):
    return subprocess.check_output(["git", *args])


def main():
    message_path, branch, expected_head = sys.argv[1:]
    additions = []
    deletions = []
    rows = git("diff", "--cached", "--raw", "-z", "--no-renames", expected_head).split(b"\0")
    for index in range(0, len(rows) - 1, 2):
        metadata = rows[index].decode().split()
        path = rows[index + 1].decode()
        old_mode, new_mode = metadata[0][1:], metadata[1]
        if old_mode not in ("000000", "100644") or new_mode not in ("000000", "100644"):
            print(f"Commit API cannot represent the file mode of {path}", file=sys.stderr)
            return 2
        if new_mode == "000000":
            deletions.append({"path": path})
        else:
            contents = git("show", f":{path}")
            additions.append({"path": path, "contents": base64.b64encode(contents).decode()})
    if not additions and not deletions:
        return 0
    headline, _, body = Path(message_path).read_text().partition("\n")
    payload = {
        "query": "mutation($input: CreateCommitOnBranchInput!) { createCommitOnBranch(input: $input) { commit { oid url } } }",
        "variables": {"input": {
            "branch": {"repositoryNameWithOwner": os.environ["GITHUB_REPOSITORY"], "branchName": branch},
            "expectedHeadOid": expected_head,
            "message": {"headline": headline, "body": body.strip()},
            "fileChanges": {"additions": additions, "deletions": deletions},
        }},
    }
    result = subprocess.run(["gh", "api", "graphql", "--input", "-"], input=json.dumps(payload), text=True, capture_output=True)
    if result.returncode:
        print(result.stdout + result.stderr, file=sys.stderr)
        return 1
    response = json.loads(result.stdout)
    if response.get("errors"):
        print(result.stdout, file=sys.stderr)
        return 1
    print(response["data"]["createCommitOnBranch"]["commit"]["oid"])
    return 0


if __name__ == "__main__":
    sys.exit(main())
