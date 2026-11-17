#!/bin/bash
# Prints the commit a GitHub Actions run built: <repo> <run-id>.
# A StartOS publish run (startos-iso.yaml) names the build's commit in its title; its
# head commit is the branch head when it was dispatched.

set -euo pipefail

read -r path commit title < <(
    gh api "repos/$1/actions/runs/$2" --jq '[.path, .head_sha, .display_title] | @tsv'
)
if [ "$path" = .github/workflows/startos-iso.yaml ]; then
    commit=${title##* }
fi
if ! [[ "$commit" =~ ^[0-9a-f]{40}$ ]]; then
    >&2 echo "Run $2 does not name the commit it built"
    exit 1
fi
echo "$commit"
