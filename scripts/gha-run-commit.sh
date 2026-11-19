#!/bin/bash
# Prints the commit a run built: <repo> <run-id>.
# StartOS Publish records its build commit in the title, not head_sha.

set -euo pipefail

IFS=$'\t' read -r path name commit title < <(
    gh api "repos/$1/actions/runs/$2" --jq '[.path, .name, .head_sha, .display_title] | @tsv'
)
if [ "$path" = .github/workflows/startos-iso.yaml ] && [ "$name" = 'StartOS Publish' ]; then
    commit=${title##* }
fi
if ! [[ "$commit" =~ ^[0-9a-f]{40}$ ]]; then
    >&2 echo "Run $2 does not name the commit it built"
    exit 1
fi
echo "$commit"
