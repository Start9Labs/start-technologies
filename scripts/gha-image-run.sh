#!/bin/bash
# Prints the newest successful image run on a branch holding an unexpired
# startos-*_<platform>.squashfs: <repo> <branch> <platform>.

set -euo pipefail

repo=$1
branch=$2
platform=$3
image="startos-*_$platform.squashfs"
run_ids=$("$(dirname "${BASH_SOURCE[0]}")/gha-artifact-run-ids.sh" "$repo" "$image" "$branch")
selected_id=$(
    for run_id in $run_ids; do
        gh api "repos/$repo/actions/runs/$run_id" || exit 1
    done | jq -sr '
        [.[] | select(.conclusion == "success" and
            (.path == ".github/workflows/startos-iso.yaml" or .path == ".github/workflows/build-products.yaml"))]
        | max_by(.created_at) | .id // empty
    '
)

if [ -z "$selected_id" ]; then
    >&2 echo "No successful run on $branch has an unexpired $image artifact"
    exit 1
fi
printf '%s\n' "$selected_id"
